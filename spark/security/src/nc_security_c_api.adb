--  NetizenCore SPARK Ada Security Core — Narrow, Versioned C/Rust ABI Body
--  Pointer marshalling layer; delegates 100% of logic to SPARK_Mode => On units.

pragma SPARK_Mode (Off);

with System; use System;
with NC_Security_Types;        use NC_Security_Types;
with NC_Security_Capabilities; use NC_Security_Capabilities;
with NC_Security_Policy;       use NC_Security_Policy;
with NC_Security_DysonSphere;  use NC_Security_DysonSphere;
with NC_Security_Filesystem;   use NC_Security_Filesystem;
with NC_Security_Crypto;       use NC_Security_Crypto;
with NC_Security_Lock;         use NC_Security_Lock;
with NC_Security_Encryption;   use NC_Security_Encryption;

package body NC_Security_C_API is

   --  Static, bounded security state (zero heap allocation).
   Global_DS   : Engine_State;
   Global_Lock : Lock_Machine_State;
   Global_FDE  : Encryption_State;

   --  Safe pointer copy helpers (bounded, NULL-checked).
   function Copy_App_Id
     (Ptr : System.Address;
      Len : size_t) return Bounded_App_Id
   is
      Res : Bounded_App_Id := Empty_App_Id;
   begin
      if Ptr = System.Null_Address or else Len = 0 or else Len > Max_App_Id_Len then
         return Empty_App_Id;
      end if;
      declare
         Src : Byte_Array (1 .. Natural (Len))
           with Import, Address => Ptr;
      begin
         Res.Len := Natural (Len);
         for I in 1 .. Res.Len loop
            Res.Data (I) := Src (I);
         end loop;
      end;
      return Res;
   end Copy_App_Id;

   function Copy_Publisher
     (Ptr : System.Address;
      Len : size_t) return Bounded_Publisher
   is
      Res : Bounded_Publisher := Empty_Publisher;
   begin
      if Ptr = System.Null_Address or else Len = 0 or else Len > Max_Publisher_Len then
         return Empty_Publisher;
      end if;
      declare
         Src : Byte_Array (1 .. Natural (Len))
           with Import, Address => Ptr;
      begin
         Res.Len := Natural (Len);
         for I in 1 .. Res.Len loop
            Res.Data (I) := Src (I);
         end loop;
      end;
      return Res;
   end Copy_Publisher;

   function Copy_Path
     (Ptr : System.Address;
      Len : size_t) return Bounded_Path
   is
      Res : Bounded_Path := (Len => 0, Data => (others => 0));
   begin
      if Ptr = System.Null_Address or else Len = 0 or else Len > Max_Path_Len then
         return Res;
      end if;
      declare
         Src : Byte_Array (1 .. Natural (Len))
           with Import, Address => Ptr;
      begin
         Res.Len := Natural (Len);
         for I in 1 .. Res.Len loop
            Res.Data (I) := Src (I);
         end loop;
      end;
      return Res;
   end Copy_Path;

   function Decision_To_U32 (D : Auth_Decision) return Unsigned_32 is
   begin
      return Unsigned_32 (Auth_Decision'Enum_Rep (D));
   end Decision_To_U32;

   --  =========================================================================
   --  1. Initialization & ABI Metadata
   --  =========================================================================

   function NC_Sec_ABI_Version return Unsigned_32 is
   begin
      return ABI_Version;
   end NC_Sec_ABI_Version;

   function NC_Sec_Init (Expected_ABI_Version : Unsigned_32) return Unsigned_32 is
   begin
      if Expected_ABI_Version /= ABI_Version then
         return 1;
      end if;
      NC_Security_DysonSphere.Initialize (Global_DS);
      Global_Lock := (Status => Lock_Disabled, Locked => False, Rec => (others => <>), Fails => 0);
      Global_FDE  := (Status => Enc_Unconfigured, Meta => (others => <>), Active_Dek => Zero_Key_256);
      return 0;
   end NC_Sec_Init;

   function NC_Sec_Is_Initialized return Unsigned_32 is
   begin
      return (if Global_DS.Initialized then 1 else 0);
   end NC_Sec_Is_Initialized;

   --  =========================================================================
   --  2. Authoritative Capabilities
   --  =========================================================================

   function NC_Sec_Caps_User_Allowed return Unsigned_64 is
   begin
      return User_Allowed_Mask;
   end NC_Sec_Caps_User_Allowed;

   function NC_Sec_Caps_System_Only return Unsigned_64 is
   begin
      return System_Only_Mask;
   end NC_Sec_Caps_System_Only;

   function NC_Sec_Caps_Clamp_User (Declared : Unsigned_64) return Unsigned_64 is
   begin
      return Clamp_User (Declared);
   end NC_Sec_Caps_Clamp_User;

   function NC_Sec_Caps_Dropped_For_User (Declared : Unsigned_64) return Unsigned_64 is
   begin
      return Dropped_For_User (Declared);
   end NC_Sec_Caps_Dropped_For_User;

   function NC_Sec_Caps_Is_Valid_Mask (Mask : Unsigned_64) return Unsigned_32 is
   begin
      return (if Is_Valid_Mask (Mask) then 1 else 0);
   end NC_Sec_Caps_Is_Valid_Mask;

   --  =========================================================================
   --  3. DysonSphere Central Authorization Engine
   --  =========================================================================

   function NC_Sec_DS_Authorize
     (Caller_Id_Ptr : System.Address;
      Caller_Id_Len : size_t;
      Requested_Cap : Unsigned_64) return Unsigned_32
   is
      Req : Auth_Request;
      Dec : Auth_Decision;
   begin
      Req.Caller_Id     := Copy_App_Id (Caller_Id_Ptr, Caller_Id_Len);
      Req.Requested_Cap := Requested_Cap;
      Check_Authorization (Global_DS, Req, Dec);
      return Decision_To_U32 (Dec);
   end NC_Sec_DS_Authorize;

   function NC_Sec_DS_Register_Package
     (App_Id_Ptr      : System.Address;
      App_Id_Len      : size_t;
      Publisher_Ptr   : System.Address;
      Publisher_Len   : size_t;
      Version_Code    : Unsigned_32;
      Payload_Hash    : System.Address;
      Requested_Caps  : Unsigned_64;
      Approved_Caps   : Unsigned_64;
      Source          : Unsigned_32;
      Is_Verified_V2  : Unsigned_32;
      Platform_Signed : Unsigned_32) return Unsigned_32
   is
      App_Id : constant Bounded_App_Id    := Copy_App_Id (App_Id_Ptr, App_Id_Len);
      Pub    : constant Bounded_Publisher := Copy_Publisher (Publisher_Ptr, Publisher_Len);
      Hash   : Hash_256 := Zero_Hash_256;
      Src_T  : constant Install_Source :=
        (if Source = 1 then Source_System_Image else Source_User_Data);
      Dec    : Auth_Decision;
   begin
      if Payload_Hash /= System.Null_Address then
         declare
            H_View : Hash_256 with Import, Address => Payload_Hash;
         begin
            Hash := H_View;
         end;
      end if;

      Register_Package
        (State           => Global_DS,
         App_Id          => App_Id,
         Publisher       => Pub,
         Version_Code    => Version_Code,
         Payload_Hash    => Hash,
         Requested_Caps  => Requested_Caps,
         Approved_Caps   => Approved_Caps,
         Source          => Src_T,
         Is_Verified_V2  => Is_Verified_V2 /= 0,
         Platform_Signed => Platform_Signed /= 0,
         Decision        => Dec);
      return Decision_To_U32 (Dec);
   end NC_Sec_DS_Register_Package;

   function NC_Sec_DS_Revoke_App
     (App_Id_Ptr : System.Address;
      App_Id_Len : size_t) return Unsigned_32
   is
      App_Id : constant Bounded_App_Id := Copy_App_Id (App_Id_Ptr, App_Id_Len);
      Ok     : Boolean;
   begin
      Revoke_App (Global_DS, App_Id, Ok);
      return (if Ok then 1 else 0);
   end NC_Sec_DS_Revoke_App;

   function NC_Sec_DS_Query_Granted_Caps
     (App_Id_Ptr : System.Address;
      App_Id_Len : size_t) return Unsigned_64
   is
      App_Id : constant Bounded_App_Id := Copy_App_Id (App_Id_Ptr, App_Id_Len);
   begin
      return Query_Granted_Caps (Global_DS, App_Id);
   end NC_Sec_DS_Query_Granted_Caps;

   function NC_Sec_DS_Denied_Count return Unsigned_64 is
   begin
      return Global_DS.Denied_Count;
   end NC_Sec_DS_Denied_Count;

   function NC_Sec_Policy_Authorize_Launch
     (Caller_Id_Ptr : System.Address;
      Caller_Id_Len : size_t;
      Caller_Caps   : Unsigned_64;
      Target_Id_Ptr : System.Address;
      Target_Id_Len : size_t) return Unsigned_32
   is
      Caller : constant Bounded_App_Id := Copy_App_Id (Caller_Id_Ptr, Caller_Id_Len);
      Target : constant Bounded_App_Id := Copy_App_Id (Target_Id_Ptr, Target_Id_Len);
   begin
      return Decision_To_U32 (Authorize_Process_Launch (Caller, Caller_Caps, Target));
   end NC_Sec_Policy_Authorize_Launch;

   function NC_Sec_Policy_Authorize_IPC
     (Sender_Id_Ptr : System.Address;
      Sender_Id_Len : size_t;
      Sender_Caps   : Unsigned_64;
      Target_Id_Ptr : System.Address;
      Target_Id_Len : size_t;
      Privileged    : Unsigned_32) return Unsigned_32
   is
      Sender : constant Bounded_App_Id := Copy_App_Id (Sender_Id_Ptr, Sender_Id_Len);
      Target : constant Bounded_App_Id := Copy_App_Id (Target_Id_Ptr, Target_Id_Len);
   begin
      return Decision_To_U32
        (Authorize_IPC (Sender, Sender_Caps, Target, Privileged /= 0));
   end NC_Sec_Policy_Authorize_IPC;

   function NC_Sec_Policy_Resolve_Tier
     (Source          : Unsigned_32;
      Is_Verified     : Unsigned_32;
      Platform_Signed : Unsigned_32) return Unsigned_32
   is
      Src_T : constant Install_Source :=
        (if Source = 1 then Source_System_Image else Source_User_Data);
      Tier  : constant Trust_Tier :=
        Resolve_Trust_Tier (Src_T, Is_Verified /= 0, Platform_Signed /= 0);
   begin
      return Unsigned_32 (Trust_Tier'Enum_Rep (Tier));
   end NC_Sec_Policy_Resolve_Tier;

   --  =========================================================================
   --  4. Authoritative Filesystem Access Policy
   --  =========================================================================

   function NC_Sec_Fs_Check
     (App_Id_Ptr : System.Address;
      App_Id_Len : size_t;
      Caps       : Unsigned_64;
      Path_Ptr   : System.Address;
      Path_Len   : size_t;
      Op         : Unsigned_32) return Unsigned_32
   is
      Caller : constant Bounded_App_Id := Copy_App_Id (App_Id_Ptr, App_Id_Len);
      Path   : constant Bounded_Path   := Copy_Path (Path_Ptr, Path_Len);
      Fs_O   : Fs_Op;
      Dec    : Fs_Decision;
   begin
      case Op is
         when 0 => Fs_O := Fs_Op_Read;
         when 1 => Fs_O := Fs_Op_Write;
         when 2 => Fs_O := Fs_Op_Delete;
         when others =>
            return Unsigned_32 (Fs_Decision'Enum_Rep (Fs_Deny_Permission));
      end case;

      Dec := Check_Access (Caller, Caps, Path, Fs_O);
      return Unsigned_32 (Fs_Decision'Enum_Rep (Dec));
   end NC_Sec_Fs_Check;

   --  =========================================================================
   --  5. Authoritative Cryptographic API
   --  =========================================================================

   procedure NC_Sec_Crypto_Sha256
     (Data_Ptr : System.Address;
      Data_Len : size_t;
      Out_Hash : System.Address)
   is
      Digest : Hash_256 := Zero_Hash_256;
      Empty  : constant Byte_Array (1 .. 0) := (others => 0);
   begin
      if Out_Hash = System.Null_Address then
         return;
      end if;

      if Data_Ptr = System.Null_Address or else Data_Len = 0 then
         Sha256_One_Shot (Empty, Digest);
      else
         declare
            Data_View : Byte_Array (0 .. Natural (Data_Len) - 1)
              with Import, Address => Data_Ptr;
         begin
            Sha256_One_Shot (Data_View, Digest);
         end;
      end if;

      declare
         Out_View : Hash_256 with Import, Address => Out_Hash;
      begin
         Out_View := Digest;
      end;
   end NC_Sec_Crypto_Sha256;

   procedure NC_Sec_Crypto_Hmac_Sha256
     (Key_Ptr  : System.Address;
      Key_Len  : size_t;
      Msg_Ptr  : System.Address;
      Msg_Len  : size_t;
      Out_Mac  : System.Address)
   is
      Mac   : Hash_256 := Zero_Hash_256;
      Empty : constant Byte_Array (1 .. 0) := (others => 0);
   begin
      if Out_Mac = System.Null_Address then
         return;
      end if;

      if (Key_Ptr = System.Null_Address or else Key_Len = 0)
        and then (Msg_Ptr = System.Null_Address or else Msg_Len = 0)
      then
         Hmac_Sha256 (Empty, Empty, Mac);
      elsif Key_Ptr = System.Null_Address or else Key_Len = 0 then
         declare
            M : Byte_Array (0 .. Natural (Msg_Len) - 1) with Import, Address => Msg_Ptr;
         begin
            Hmac_Sha256 (Empty, M, Mac);
         end;
      elsif Msg_Ptr = System.Null_Address or else Msg_Len = 0 then
         declare
            K : Byte_Array (0 .. Natural (Key_Len) - 1) with Import, Address => Key_Ptr;
         begin
            Hmac_Sha256 (K, Empty, Mac);
         end;
      else
         declare
            K : Byte_Array (0 .. Natural (Key_Len) - 1) with Import, Address => Key_Ptr;
            M : Byte_Array (0 .. Natural (Msg_Len) - 1) with Import, Address => Msg_Ptr;
         begin
            Hmac_Sha256 (K, M, Mac);
         end;
      end if;

      declare
         Out_View : Hash_256 with Import, Address => Out_Mac;
      begin
         Out_View := Mac;
      end;
   end NC_Sec_Crypto_Hmac_Sha256;

   function NC_Sec_Crypto_Pbkdf2_Sha256
     (Pass_Ptr   : System.Address;
      Pass_Len   : size_t;
      Salt_Ptr   : System.Address;
      Salt_Len   : size_t;
      Iterations : Unsigned_32;
      Out_Ptr    : System.Address;
      Out_Len    : size_t) return Unsigned_32
   is
      Empty : constant Byte_Array (1 .. 0) := (others => 0);
   begin
      if Out_Ptr = System.Null_Address
        or else Out_Len = 0
        or else Out_Len > 1024
        or else Iterations = 0
      then
         return 1;
      end if;

      declare
         Out_View : Byte_Array (0 .. Natural (Out_Len) - 1) with Import, Address => Out_Ptr;
      begin
         if (Pass_Ptr = System.Null_Address or else Pass_Len = 0)
           and then (Salt_Ptr = System.Null_Address or else Salt_Len = 0)
         then
            Pbkdf2_Hmac_Sha256 (Empty, Empty, Iterations, Out_View);
         elsif Pass_Ptr = System.Null_Address or else Pass_Len = 0 then
            declare
               S : Byte_Array (0 .. Natural (Salt_Len) - 1) with Import, Address => Salt_Ptr;
            begin
               Pbkdf2_Hmac_Sha256 (Empty, S, Iterations, Out_View);
            end;
         elsif Salt_Ptr = System.Null_Address or else Salt_Len = 0 then
            declare
               P : Byte_Array (0 .. Natural (Pass_Len) - 1) with Import, Address => Pass_Ptr;
            begin
               Pbkdf2_Hmac_Sha256 (P, Empty, Iterations, Out_View);
            end;
         else
            declare
               P : Byte_Array (0 .. Natural (Pass_Len) - 1) with Import, Address => Pass_Ptr;
               S : Byte_Array (0 .. Natural (Salt_Len) - 1) with Import, Address => Salt_Ptr;
            begin
               Pbkdf2_Hmac_Sha256 (P, S, Iterations, Out_View);
            end;
         end if;
      end;
      return 0;
   end NC_Sec_Crypto_Pbkdf2_Sha256;

   function NC_Sec_Crypto_Const_Time_Eq
     (A_Ptr : System.Address;
      B_Ptr : System.Address;
      Len   : size_t) return Unsigned_32
   is
   begin
      if Len = 0 then
         return 1;
      end if;
      if A_Ptr = System.Null_Address or else B_Ptr = System.Null_Address then
         return 0;
      end if;
      declare
         A : Byte_Array (0 .. Natural (Len) - 1) with Import, Address => A_Ptr;
         B : Byte_Array (0 .. Natural (Len) - 1) with Import, Address => B_Ptr;
      begin
         return (if Constant_Time_Eq (A, B) then 1 else 0);
      end;
   end NC_Sec_Crypto_Const_Time_Eq;

   procedure NC_Sec_Crypto_Secure_Zero
     (Buf_Ptr : System.Address;
      Buf_Len : size_t)
   is
   begin
      if Buf_Ptr = System.Null_Address or else Buf_Len = 0 then
         return;
      end if;
      declare
         Buf : Byte_Array (0 .. Natural (Buf_Len) - 1) with Import, Address => Buf_Ptr;
      begin
         Secure_Zero (Buf);
      end;
   end NC_Sec_Crypto_Secure_Zero;

   function NC_Sec_Crypto_Aead_Encrypt
     (Key_Ptr    : System.Address;
      Nonce_Ptr  : System.Address;
      Aad_Ptr    : System.Address;
      Aad_Len    : size_t;
      Pt_Ptr     : System.Address;
      Pt_Len     : size_t;
      Ct_Out_Ptr : System.Address;
      Tag_Out    : System.Address) return Unsigned_32
   is
      Empty : constant Byte_Array (1 .. 0) := (others => 0);
   begin
      if Key_Ptr = System.Null_Address
        or else Nonce_Ptr = System.Null_Address
        or else Tag_Out = System.Null_Address
        or else Pt_Len > 65536
        or else Aad_Len > 4096
        or else (Pt_Len > 0 and then (Pt_Ptr = System.Null_Address or else Ct_Out_Ptr = System.Null_Address))
      then
         return 1;
      end if;

      declare
         Key   : Key_256  with Import, Address => Key_Ptr;
         Nonce : Nonce_96 with Import, Address => Nonce_Ptr;
         Tag   : Tag_128  with Import, Address => Tag_Out;
      begin
         if Pt_Len = 0 and then Aad_Len = 0 then
            declare
               Ct_Empty : Byte_Array (1 .. 0);
            begin
               ChaCha20_Poly1305_Encrypt (Key, Nonce, Empty, Empty, Ct_Empty, Tag);
            end;
         elsif Pt_Len = 0 then
            declare
               Aad      : Byte_Array (0 .. Natural (Aad_Len) - 1) with Import, Address => Aad_Ptr;
               Ct_Empty : Byte_Array (1 .. 0);
            begin
               ChaCha20_Poly1305_Encrypt (Key, Nonce, Aad, Empty, Ct_Empty, Tag);
            end;
         elsif Aad_Len = 0 then
            declare
               Pt : Byte_Array (0 .. Natural (Pt_Len) - 1) with Import, Address => Pt_Ptr;
               Ct : Byte_Array (0 .. Natural (Pt_Len) - 1) with Import, Address => Ct_Out_Ptr;
            begin
               ChaCha20_Poly1305_Encrypt (Key, Nonce, Empty, Pt, Ct, Tag);
            end;
         else
            declare
               Aad : Byte_Array (0 .. Natural (Aad_Len) - 1) with Import, Address => Aad_Ptr;
               Pt  : Byte_Array (0 .. Natural (Pt_Len) - 1)  with Import, Address => Pt_Ptr;
               Ct  : Byte_Array (0 .. Natural (Pt_Len) - 1)  with Import, Address => Ct_Out_Ptr;
            begin
               ChaCha20_Poly1305_Encrypt (Key, Nonce, Aad, Pt, Ct, Tag);
            end;
         end if;
      end;
      return 0;
   end NC_Sec_Crypto_Aead_Encrypt;

   function NC_Sec_Crypto_Aead_Decrypt
     (Key_Ptr    : System.Address;
      Nonce_Ptr  : System.Address;
      Aad_Ptr    : System.Address;
      Aad_Len    : size_t;
      Ct_Ptr     : System.Address;
      Ct_Len     : size_t;
      Tag_Ptr    : System.Address;
      Pt_Out_Ptr : System.Address) return Unsigned_32
   is
      Empty : constant Byte_Array (1 .. 0) := (others => 0);
      Ok    : Boolean := False;
   begin
      if Key_Ptr = System.Null_Address
        or else Nonce_Ptr = System.Null_Address
        or else Tag_Ptr = System.Null_Address
        or else Ct_Len > 65536
        or else Aad_Len > 4096
        or else (Ct_Len > 0 and then (Ct_Ptr = System.Null_Address or else Pt_Out_Ptr = System.Null_Address))
      then
         return 1;
      end if;

      declare
         Key   : Key_256  with Import, Address => Key_Ptr;
         Nonce : Nonce_96 with Import, Address => Nonce_Ptr;
         Tag   : Tag_128  with Import, Address => Tag_Ptr;
      begin
         if Ct_Len = 0 and then Aad_Len = 0 then
            declare
               Pt_Empty : Byte_Array (1 .. 0);
            begin
               ChaCha20_Poly1305_Decrypt (Key, Nonce, Empty, Empty, Tag, Pt_Empty, Ok);
            end;
         elsif Ct_Len = 0 then
            declare
               Aad      : Byte_Array (0 .. Natural (Aad_Len) - 1) with Import, Address => Aad_Ptr;
               Pt_Empty : Byte_Array (1 .. 0);
            begin
               ChaCha20_Poly1305_Decrypt (Key, Nonce, Aad, Empty, Tag, Pt_Empty, Ok);
            end;
         elsif Aad_Len = 0 then
            declare
               Ct : Byte_Array (0 .. Natural (Ct_Len) - 1) with Import, Address => Ct_Ptr;
               Pt : Byte_Array (0 .. Natural (Ct_Len) - 1) with Import, Address => Pt_Out_Ptr;
            begin
               ChaCha20_Poly1305_Decrypt (Key, Nonce, Empty, Ct, Tag, Pt, Ok);
            end;
         else
            declare
               Aad : Byte_Array (0 .. Natural (Aad_Len) - 1) with Import, Address => Aad_Ptr;
               Ct  : Byte_Array (0 .. Natural (Ct_Len) - 1)  with Import, Address => Ct_Ptr;
               Pt  : Byte_Array (0 .. Natural (Ct_Len) - 1)  with Import, Address => Pt_Out_Ptr;
            begin
               ChaCha20_Poly1305_Decrypt (Key, Nonce, Aad, Ct, Tag, Pt, Ok);
            end;
         end if;
      end;
      return (if Ok then 0 else 1);
   end NC_Sec_Crypto_Aead_Decrypt;

   --  =========================================================================
   --  6. Authoritative Screen Lock & Credential State Machine
   --  =========================================================================

   function NC_Sec_Lock_Secret_Valid
     (Kind       : Unsigned_8;
      Secret_Ptr : System.Address;
      Secret_Len : size_t) return Unsigned_32
   is
   begin
      if Secret_Ptr = System.Null_Address or else Secret_Len = 0 or else Secret_Len > Max_Secret_Len then
         return 0;
      end if;
      declare
         S : Byte_Array (0 .. Natural (Secret_Len) - 1) with Import, Address => Secret_Ptr;
      begin
         return (if Secret_Is_Valid (Kind, S) then 1 else 0);
      end;
   end NC_Sec_Lock_Secret_Valid;

   function NC_Sec_Lock_Lockout_Secs (Fails : Unsigned_32) return Unsigned_32 is
   begin
      return Lockout_Seconds_For (Fails);
   end NC_Sec_Lock_Lockout_Secs;

   function NC_Sec_Lock_Derive_Sectors
     (Kind            : Unsigned_8;
      Secret_Ptr      : System.Address;
      Secret_Len      : size_t;
      Salt_Ptr        : System.Address;
      Iterations      : Unsigned_32;
      Fails           : Unsigned_32;
      Cred_Sector_Out : System.Address;
      Mark_Sector_Out : System.Address) return Unsigned_32
   is
      Rec    : Lock_Record;
      Cred   : Sector_512;
      Mark   : Sector_512;
      Digest : Hash_256;
   begin
      if Secret_Ptr = System.Null_Address
        or else Salt_Ptr = System.Null_Address
        or else Cred_Sector_Out = System.Null_Address
        or else Mark_Sector_Out = System.Null_Address
        or else Secret_Len = 0
        or else Secret_Len > Max_Secret_Len
        or else Iterations = 0
      then
         return 1;
      end if;

      declare
         Sec  : Byte_Array (0 .. Natural (Secret_Len) - 1) with Import, Address => Secret_Ptr;
         Salt : Salt_128 with Import, Address => Salt_Ptr;
      begin
         if not Secret_Is_Valid (Kind, Sec) then
            return 1;
         end if;
         Derive_Record (Kind, Sec, Salt, Iterations, Rec);
      end;

      Record_To_Sector (Rec, Cred);
      Sha256_One_Shot (Cred, Digest);
      Marker_To_Sector ((Enabled => True, Fails => Fails, Credential_Digest => Digest), Mark);

      declare
         Cred_View : Sector_512 with Import, Address => Cred_Sector_Out;
         Mark_View : Sector_512 with Import, Address => Mark_Sector_Out;
      begin
         Cred_View := Cred;
         Mark_View := Mark;
      end;
      return 0;
   end NC_Sec_Lock_Derive_Sectors;

   function NC_Sec_Lock_Evaluate_Sectors
     (Cred_Sector_Ptr : System.Address;
      Mark_Sector_Ptr : System.Address;
      Out_Kind        : access Unsigned_8;
      Out_Iterations  : access Unsigned_32;
      Out_Fails       : access Unsigned_32;
      Out_Salt        : System.Address;
      Out_Hash        : System.Address) return Unsigned_32
   is
      Status : Lock_Disk_Status;
      Rec    : Lock_Record;
      Fails  : Unsigned_32;
   begin
      if Cred_Sector_Ptr = System.Null_Address or else Mark_Sector_Ptr = System.Null_Address then
         return Unsigned_32 (Lock_Disk_Status'Enum_Rep (Lock_Destroyed));
      end if;

      declare
         Cred : Sector_512 with Import, Address => Cred_Sector_Ptr;
         Mark : Sector_512 with Import, Address => Mark_Sector_Ptr;
      begin
         Evaluate_Sectors (Cred, Mark, Status, Rec, Fails);
      end;

      if Out_Kind /= null then
         Out_Kind.all := Rec.Kind;
      end if;
      if Out_Iterations /= null then
         Out_Iterations.all := Rec.Iterations;
      end if;
      if Out_Fails /= null then
         Out_Fails.all := Fails;
      end if;
      if Out_Salt /= System.Null_Address then
         declare
            S_Out : Salt_128 with Import, Address => Out_Salt;
         begin
            S_Out := Rec.Salt;
         end;
      end if;
      if Out_Hash /= System.Null_Address then
         declare
            H_Out : Hash_256 with Import, Address => Out_Hash;
         begin
            H_Out := Rec.Hash;
         end;
      end if;

      return Unsigned_32 (Lock_Disk_Status'Enum_Rep (Status));
   end NC_Sec_Lock_Evaluate_Sectors;

   function NC_Sec_Lock_Verify_Sector
     (Cred_Sector_Ptr : System.Address;
      Secret_Ptr      : System.Address;
      Secret_Len      : size_t) return Unsigned_32
   is
      Rec   : Lock_Record;
      Valid : Boolean;
      Match : Boolean := False;
   begin
      if Cred_Sector_Ptr = System.Null_Address
        or else Secret_Ptr = System.Null_Address
        or else Secret_Len = 0
        or else Secret_Len > Max_Secret_Len
      then
         return 0;
      end if;

      declare
         Cred : Sector_512 with Import, Address => Cred_Sector_Ptr;
         Sec  : Byte_Array (0 .. Natural (Secret_Len) - 1) with Import, Address => Secret_Ptr;
      begin
         Record_From_Sector (Cred, Rec, Valid);
         if Valid then
            Verify_Record (Rec, Sec, Match);
         end if;
      end;
      return (if Match then 1 else 0);
   end NC_Sec_Lock_Verify_Sector;

   function NC_Sec_Lock_Load_State
     (Cred_Sector_Ptr : System.Address;
      Mark_Sector_Ptr : System.Address;
      Lock_On_Boot    : Unsigned_32) return Unsigned_32
   is
   begin
      if Cred_Sector_Ptr = System.Null_Address or else Mark_Sector_Ptr = System.Null_Address then
         Global_Lock := (Status => Lock_Destroyed, Locked => True, Rec => (others => <>), Fails => 0);
         return Unsigned_32 (Lock_Disk_Status'Enum_Rep (Lock_Destroyed));
      end if;

      declare
         Cred : Sector_512 with Import, Address => Cred_Sector_Ptr;
         Mark : Sector_512 with Import, Address => Mark_Sector_Ptr;
      begin
         Load_Machine (Cred, Mark, Lock_On_Boot /= 0, Global_Lock);
      end;
      return Unsigned_32 (Lock_Disk_Status'Enum_Rep (Global_Lock.Status));
   end NC_Sec_Lock_Load_State;

   function NC_Sec_Lock_Is_Locked return Unsigned_32 is
   begin
      return (if Global_Lock.Locked then 1 else 0);
   end NC_Sec_Lock_Is_Locked;

   function NC_Sec_Lock_Status return Unsigned_32 is
   begin
      return Unsigned_32 (Lock_Disk_Status'Enum_Rep (Global_Lock.Status));
   end NC_Sec_Lock_Status;

   function NC_Sec_Lock_Now return Unsigned_32 is
      Ok : Boolean;
   begin
      Lock_Now (Global_Lock, Ok);
      return (if Ok then 1 else 0);
   end NC_Sec_Lock_Now;

   function NC_Sec_Lock_Authenticate
     (Secret_Ptr     : System.Address;
      Secret_Len     : size_t;
      Lockout_Active : Unsigned_32;
      Out_Delay_Secs : access Unsigned_32) return Unsigned_32
   is
      Res   : Lock_Auth_Result;
      Delay_S : Unsigned_32 := 0;
      Empty : constant Byte_Array (1 .. 0) := (others => 0);
   begin
      if Secret_Len > Max_Secret_Len then
         if Out_Delay_Secs /= null then
            Out_Delay_Secs.all := 0;
         end if;
         return Unsigned_32 (Lock_Auth_Result'Enum_Rep (Auth_Res_Invalid_Secret));
      end if;

      if Secret_Ptr = System.Null_Address or else Secret_Len = 0 then
         Authenticate (Global_Lock, Empty, Lockout_Active /= 0, Res, Delay_S);
      else
         declare
            Sec : Byte_Array (0 .. Natural (Secret_Len) - 1) with Import, Address => Secret_Ptr;
         begin
            Authenticate (Global_Lock, Sec, Lockout_Active /= 0, Res, Delay_S);
         end;
      end if;

      if Out_Delay_Secs /= null then
         Out_Delay_Secs.all := Delay_S;
      end if;
      return Unsigned_32 (Lock_Auth_Result'Enum_Rep (Res));
   end NC_Sec_Lock_Authenticate;

   --  =========================================================================
   --  7. Authoritative Full-Data Encryption (/data) State Machine
   --  =========================================================================

   function NC_Sec_Fde_Load_Metadata
     (Meta_Sector_Ptr : System.Address) return Unsigned_32
   is
   begin
      if Meta_Sector_Ptr = System.Null_Address then
         Global_FDE := (Status => Enc_Corrupted, Meta => (others => <>), Active_Dek => Zero_Key_256);
         return Unsigned_32 (Encryption_Status'Enum_Rep (Enc_Corrupted));
      end if;
      declare
         Meta : Sector_512 with Import, Address => Meta_Sector_Ptr;
      begin
         Load_Metadata_Sector (Meta, Global_FDE);
      end;
      return Unsigned_32 (Encryption_Status'Enum_Rep (Global_FDE.Status));
   end NC_Sec_Fde_Load_Metadata;

   function NC_Sec_Fde_Provision
     (Pass_Ptr        : System.Address;
      Pass_Len        : size_t;
      Salt_Ptr        : System.Address;
      Nonce_Ptr       : System.Address;
      Raw_Dek_Ptr     : System.Address;
      Iterations      : Unsigned_32;
      Meta_Sector_Out : System.Address) return Unsigned_32
   is
      Ok : Boolean := False;
   begin
      if Pass_Ptr = System.Null_Address
        or else Salt_Ptr = System.Null_Address
        or else Nonce_Ptr = System.Null_Address
        or else Raw_Dek_Ptr = System.Null_Address
        or else Meta_Sector_Out = System.Null_Address
        or else Pass_Len = 0
        or else Pass_Len > Max_Secret_Len
      then
         return 1;
      end if;

      declare
         Pass  : Byte_Array (0 .. Natural (Pass_Len) - 1) with Import, Address => Pass_Ptr;
         Salt  : Salt_128   with Import, Address => Salt_Ptr;
         Nonce : Nonce_96   with Import, Address => Nonce_Ptr;
         Dek   : Key_256    with Import, Address => Raw_Dek_Ptr;
         Meta  : Sector_512 with Import, Address => Meta_Sector_Out;
      begin
         Provision_Volume (Pass, Salt, Nonce, Dek, Iterations, Global_FDE, Meta, Ok);
      end;
      return (if Ok then 0 else 1);
   end NC_Sec_Fde_Provision;

   function NC_Sec_Fde_Unlock
     (Pass_Ptr : System.Address;
      Pass_Len : size_t) return Unsigned_32
   is
      Ok : Boolean := False;
   begin
      if Pass_Ptr = System.Null_Address or else Pass_Len = 0 or else Pass_Len > Max_Secret_Len then
         return 1;
      end if;
      declare
         Pass : Byte_Array (0 .. Natural (Pass_Len) - 1) with Import, Address => Pass_Ptr;
      begin
         Unlock_Volume (Global_FDE, Pass, Ok);
      end;
      return (if Ok then 0 else 1);
   end NC_Sec_Fde_Unlock;

   procedure NC_Sec_Fde_Lock is
   begin
      Lock_Volume (Global_FDE);
   end NC_Sec_Fde_Lock;

   function NC_Sec_Fde_Status return Unsigned_32 is
   begin
      return Unsigned_32 (Encryption_Status'Enum_Rep (Global_FDE.Status));
   end NC_Sec_Fde_Status;

   function NC_Sec_Fde_Encrypt_Block
     (Block_Id   : Unsigned_64;
      Nonce_Ptr  : System.Address;
      Pt_Ptr     : System.Address;
      Pt_Len     : size_t;
      Ct_Out_Ptr : System.Address;
      Tag_Out    : System.Address) return Unsigned_32
   is
      Ok : Boolean := False;
   begin
      if Nonce_Ptr = System.Null_Address
        or else Pt_Ptr = System.Null_Address
        or else Ct_Out_Ptr = System.Null_Address
        or else Tag_Out = System.Null_Address
        or else Pt_Len = 0
        or else Pt_Len > 4096
      then
         return 1;
      end if;
      declare
         Nonce : Nonce_96 with Import, Address => Nonce_Ptr;
         Pt    : Byte_Array (0 .. Natural (Pt_Len) - 1) with Import, Address => Pt_Ptr;
         Ct    : Byte_Array (0 .. Natural (Pt_Len) - 1) with Import, Address => Ct_Out_Ptr;
         Tag   : Tag_128  with Import, Address => Tag_Out;
      begin
         Encrypt_Block (Global_FDE, Block_Id, Nonce, Pt, Ct, Tag, Ok);
      end;
      return (if Ok then 0 else 1);
   end NC_Sec_Fde_Encrypt_Block;

   function NC_Sec_Fde_Decrypt_Block
     (Block_Id   : Unsigned_64;
      Nonce_Ptr  : System.Address;
      Ct_Ptr     : System.Address;
      Ct_Len     : size_t;
      Tag_Ptr    : System.Address;
      Pt_Out_Ptr : System.Address) return Unsigned_32
   is
      Ok : Boolean := False;
   begin
      if Nonce_Ptr = System.Null_Address
        or else Ct_Ptr = System.Null_Address
        or else Tag_Ptr = System.Null_Address
        or else Pt_Out_Ptr = System.Null_Address
        or else Ct_Len = 0
        or else Ct_Len > 4096
      then
         return 1;
      end if;
      declare
         Nonce : Nonce_96 with Import, Address => Nonce_Ptr;
         Ct    : Byte_Array (0 .. Natural (Ct_Len) - 1) with Import, Address => Ct_Ptr;
         Tag   : Tag_128  with Import, Address => Tag_Ptr;
         Pt    : Byte_Array (0 .. Natural (Ct_Len) - 1) with Import, Address => Pt_Out_Ptr;
      begin
         Decrypt_Block (Global_FDE, Block_Id, Nonce, Ct, Tag, Pt, Ok);
      end;
      return (if Ok then 0 else 1);
   end NC_Sec_Fde_Decrypt_Block;

end NC_Security_C_API;
