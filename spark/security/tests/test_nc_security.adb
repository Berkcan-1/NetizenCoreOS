--  NetizenCore SPARK Security Core — Standalone Verification & KAT Test Suite
with Ada.Text_IO;              use Ada.Text_IO;
with Interfaces;               use Interfaces;
with NC_Security_Types;        use NC_Security_Types;
with NC_Security_Capabilities; use NC_Security_Capabilities;
with NC_Security_DysonSphere;  use NC_Security_DysonSphere;
with NC_Security_Filesystem;   use NC_Security_Filesystem;
with NC_Security_Crypto;       use NC_Security_Crypto;
with NC_Security_Lock;         use NC_Security_Lock;
with NC_Security_Encryption;   use NC_Security_Encryption;

procedure Test_NC_Security is
   Failed : Natural := 0;

   procedure Check (Cond : Boolean; Name : String) is
   begin
      if Cond then
         Put_Line ("[PASS] " & Name);
      else
         Put_Line ("[FAIL] " & Name);
         Failed := Failed + 1;
      end if;
   end Check;

   function Str_To_Bytes (S : String) return Byte_Array is
      Res : Byte_Array (0 .. S'Length - 1);
   begin
      for I in 0 .. S'Length - 1 loop
         Res (I) := Character'Pos (S (S'First + I));
      end loop;
      return Res;
   end Str_To_Bytes;

   function Hex_Val (C : Character) return Byte is
   begin
      if C in '0' .. '9' then
         return Character'Pos (C) - Character'Pos ('0');
      elsif C in 'a' .. 'f' then
         return Character'Pos (C) - Character'Pos ('a') + 10;
      else
         return Character'Pos (C) - Character'Pos ('A') + 10;
      end if;
   end Hex_Val;

   function Hex_To_Bytes (H : String) return Byte_Array is
      Res : Byte_Array (0 .. H'Length / 2 - 1);
   begin
      for I in 0 .. Res'Length - 1 loop
         Res (I) := Shift_Left (Hex_Val (H (H'First + I * 2)), 4)
                    or Hex_Val (H (H'First + I * 2 + 1));
      end loop;
      return Res;
   end Hex_To_Bytes;

   function Make_Path (S : String) return Bounded_Path is
      P : Bounded_Path := (Len => S'Length, Data => (others => 0));
   begin
      for I in 1 .. S'Length loop
         P.Data (I) := Character'Pos (S (S'First + I - 1));
      end loop;
      return P;
   end Make_Path;

begin
   --  1. SHA-256 FIPS 180-4 KATs
   declare
      D : Hash_256;
      Empty : constant Byte_Array (1 .. 0) := (others => 0);
   begin
      Sha256_One_Shot (Empty, D);
      Check (D = Hex_To_Bytes ("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
             "SHA-256 KAT empty");

      Sha256_One_Shot (Str_To_Bytes ("abc"), D);
      Check (D = Hex_To_Bytes ("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
             "SHA-256 KAT abc");

      Sha256_One_Shot (Str_To_Bytes ("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"), D);
      Check (D = Hex_To_Bytes ("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"),
             "SHA-256 KAT 448-bit");
   end;

   --  2. HMAC-SHA256 RFC 4231 Test Case 2
   declare
      Mac : Hash_256;
   begin
      Hmac_Sha256 (Str_To_Bytes ("Jefe"), Str_To_Bytes ("what do ya want for nothing?"), Mac);
      Check (Mac = Hex_To_Bytes ("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"),
             "HMAC-SHA256 RFC 4231 TC2");
   end;

   --  3. PBKDF2-HMAC-SHA256 KAT (1 iteration & 2 iterations)
   declare
      Out1 : Byte_Array (0 .. 31);
      Out2 : Byte_Array (0 .. 31);
   begin
      Pbkdf2_Hmac_Sha256 (Str_To_Bytes ("password"), Str_To_Bytes ("salt"), 1, Out1);
      Check (Out1 = Hex_To_Bytes ("120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"),
             "PBKDF2-HMAC-SHA256 iter=1");

      Pbkdf2_Hmac_Sha256 (Str_To_Bytes ("password"), Str_To_Bytes ("salt"), 2, Out2);
      Check (Out2 = Hex_To_Bytes ("ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"),
             "PBKDF2-HMAC-SHA256 iter=2");
   end;

   --  4. RFC 8439 Section 2.8.2 ChaCha20-Poly1305 AEAD Test Vector
   declare
      Key : constant Key_256 := Hex_To_Bytes
        ("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
      Nonce : constant Nonce_96 := Hex_To_Bytes ("070000004041424344454647");
      AAD : constant Byte_Array := Hex_To_Bytes ("50515253c0c1c2c3c4c5c6c7");
      PT  : constant Byte_Array := Str_To_Bytes
        ("Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.");
      Expected_CT : constant Byte_Array := Hex_To_Bytes
        ("d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6"
         & "3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36"
         & "92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc"
         & "3ff4def08e4b7a9de576d26586cec64b6116");
      Expected_Tag : constant Tag_128 := Hex_To_Bytes ("1ae10b594f09e26a7e902ecbd0600691");
      CT  : Byte_Array (0 .. PT'Length - 1);
      Dec : Byte_Array (0 .. PT'Length - 1);
      Tag : Tag_128;
      Ok  : Boolean;
   begin
      ChaCha20_Poly1305_Encrypt (Key, Nonce, AAD, PT, CT, Tag);
      Check (CT = Expected_CT, "ChaCha20-Poly1305 RFC 8439 ciphertext");
      Check (Tag = Expected_Tag, "ChaCha20-Poly1305 RFC 8439 tag");

      ChaCha20_Poly1305_Decrypt (Key, Nonce, AAD, CT, Tag, Dec, Ok);
      Check (Ok and then Dec = PT, "ChaCha20-Poly1305 RFC 8439 decrypt");

      --  Tamper tag -> must fail and zeroize plaintext
      Tag (0) := Tag (0) xor 1;
      ChaCha20_Poly1305_Decrypt (Key, Nonce, AAD, CT, Tag, Dec, Ok);
      Check (not Ok and then Dec (0) = 0 and then Dec (Dec'Last) = 0,
             "ChaCha20-Poly1305 tamper fails closed & zeroizes");
   end;

   --  5. DysonSphere Authorization & Security Invariants
   declare
      DS  : Engine_State;
      Dec : Auth_Decision;
      Rev : Boolean;
   begin
      Initialize (DS);
      --  Unknown app denied
      Check_Authorization (DS, (Make_App_Id ("evil.app"), Cap_Fs_User_Read), Dec);
      Check (Dec = Auth_Deny_Unknown_App, "DysonSphere unknown app denied");

      --  Built-in least privilege
      Check_Authorization (DS, (Make_App_Id ("nc.stopwatch"), Cap_Fs_User_Read), Dec);
      Check (Dec = Auth_Deny_Capability_Not_Granted, "DysonSphere nc.stopwatch least privilege");
      Check_Authorization (DS, (Make_App_Id ("nc.settings"), Cap_Security_Manage), Dec);
      Check (Dec = Auth_Allow, "DysonSphere nc.settings security.manage allowed");

      --  Impersonation blocked
      Register_Package
        (DS, Make_App_Id ("nc.settings"), Make_Publisher ("attacker"), 2,
         Zero_Hash_256, Cap_Security_Manage, Cap_Security_Manage,
         Source_User_Data, True, False, Dec);
      Check (Dec = Auth_Deny_Impersonation, "DysonSphere built-in impersonation blocked");

      Register_Package
        (DS, Make_App_Id ("nc.keyboard"), Make_Publisher ("attacker"), 2,
         Zero_Hash_256, Cap_Settings_Read, Cap_Settings_Read,
         Source_User_Data, True, False, Dec);
      Check (Dec = Auth_Deny_Impersonation, "DysonSphere nc.keyboard impersonation blocked");

      Register_Package
        (DS, Make_App_Id ("nc.camera"), Make_Publisher ("attacker"), 2,
         Zero_Hash_256, Cap_Settings_Read, Cap_Settings_Read,
         Source_User_Data, True, False, Dec);
      Check (Dec = Auth_Deny_Impersonation, "DysonSphere nc.camera impersonation blocked");

      --  Register valid third-party app with version 5; request system cap -> clamped!
      Register_Package
        (DS, Make_App_Id ("com.example.notes"), Make_Publisher ("pub1"), 5,
         Zero_Hash_256, Cap_Fs_User_Read or Cap_Security_Manage, Cap_Fs_User_Read or Cap_Security_Manage,
         Source_User_Data, True, False, Dec);
      Check (Dec = Auth_Allow, "DysonSphere register third-party app");
      Check_Authorization (DS, (Make_App_Id ("com.example.notes"), Cap_Fs_User_Read), Dec);
      Check (Dec = Auth_Allow, "DysonSphere third-party allowed user cap");
      Check_Authorization (DS, (Make_App_Id ("com.example.notes"), Cap_Security_Manage), Dec);
      Check (Dec = Auth_Deny_Capability_Not_Granted, "DysonSphere third-party denied system cap");

      --  Rollback to version 4 rejected
      Register_Package
        (DS, Make_App_Id ("com.example.notes"), Make_Publisher ("pub1"), 4,
         Zero_Hash_256, Cap_Fs_User_Read, Cap_Fs_User_Read,
         Source_User_Data, True, False, Dec);
      Check (Dec = Auth_Deny_Rollback_Version, "DysonSphere rollback rejected");

      --  Revocation immediately blocks access
      Revoke_App (DS, Make_App_Id ("com.example.notes"), Rev);
      Check (Rev, "DysonSphere revoke succeeds");
      Check_Authorization (DS, (Make_App_Id ("com.example.notes"), Cap_Fs_User_Read), Dec);
      Check (Dec = Auth_Deny_Revoked_App, "DysonSphere revoked app denied");
   end;

   --  6. Filesystem Policy Invariants
   Check (Check_Access (Make_App_Id ("demo.a"), User_Allowed_Mask, Make_Path ("/data/../system"), Fs_Op_Read) = Fs_Deny_Bad_Path,
          "FS policy rejects .. traversal");
   Check (Check_Access (Make_App_Id ("demo.a"), User_Allowed_Mask, Make_Path ("/data/appdata/demo.a/save.bin"), Fs_Op_Write) = Fs_Allow,
          "FS policy allows own sandbox");
   Check (Check_Access (Make_App_Id ("demo.a"), User_Allowed_Mask, Make_Path ("/data/appdata/demo.b/save.bin"), Fs_Op_Read) = Fs_Deny_Permission,
          "FS policy blocks other app sandbox");
   Check (Check_Access (Make_App_Id ("nc.files"), Cap_Fs_All_Read or Cap_Fs_All_Write, Make_Path ("/system/packages"), Fs_Op_Write) = Fs_Deny_Read_Only,
          "FS policy /system read-only");
   Check (Check_Access (Make_App_Id ("nc.files"), Cap_Fs_All_Read or Cap_Fs_All_Write, Make_Path ("/data/appdata/nc.security/permissions.xml"), Fs_Op_Write) = Fs_Deny_Permission,
          "FS policy blocks VFS write to nc.security even for FS_ALL_WRITE");
   Check (Check_Access (Make_App_Id ("nc.files"), Cap_Fs_All_Read or Cap_Fs_All_Write, Make_Path ("/data/appdata/nc.security/permissions.xml"), Fs_Op_Read) = Fs_Deny_Permission,
          "FS policy blocks VFS read of nc.security even for FS_ALL_READ");
   Check (Check_Access (Make_App_Id ("nc.contacts"), Cap_Fs_All_Read or Cap_Fs_All_Write, Make_Path ("/data/appdata/nc.contacts.db/contacts.xml"), Fs_Op_Read) = Fs_Deny_Permission,
          "FS policy blocks direct VFS bypass of system contacts database");

   --  7. Lock State Machine & Fail-Closed Destroyed Handling
   declare
      Rec    : Lock_Record;
      Cred   : Sector_512;
      Mark   : Sector_512;
      Digest : Hash_256;
      Mach   : Lock_Machine_State;
      Res    : Lock_Auth_Result;
      Del    : Unsigned_32;
      Salt   : constant Salt_128 := (others => 16#42#);
   begin
      Derive_Record (Kind_Pin, Str_To_Bytes ("1357"), Salt, 100, Rec);
      Record_To_Sector (Rec, Cred);
      Sha256_One_Shot (Cred, Digest);
      Marker_To_Sector ((Enabled => True, Fails => 0, Credential_Digest => Digest), Mark);

      Load_Machine (Cred, Mark, True, Mach);
      Check (Mach.Status = Lock_Enabled and then Mach.Locked, "Lock loads enabled & locked");

      Authenticate (Mach, Str_To_Bytes ("0000"), False, Res, Del);
      Check (Res = Auth_Res_Invalid_Secret and then Mach.Locked, "Lock wrong PIN stays locked");

      Authenticate (Mach, Str_To_Bytes ("1357"), False, Res, Del);
      Check (Res = Auth_Res_Unlocked and then not Mach.Locked, "Lock right PIN unlocks");

      --  Tamper credential sector -> MUST fail closed (Destroyed AND Locked!)
      Cred (100) := Cred (100) xor 16#FF#;
      Load_Machine (Cred, Mark, False, Mach);
      Check (Mach.Status = Lock_Destroyed and then Mach.Locked, "Lock tampered sector fails closed (Locked=True)");
   end;

   --  8. Full-Data Encryption (/data) State Machine
   declare
      FDE   : Encryption_State;
      Meta  : Sector_512;
      Salt  : constant Salt_128 := (others => 16#11#);
      Nonce : constant Nonce_96 := (others => 16#22#);
      Dek   : constant Key_256  := (others => 16#33#);
      Ok    : Boolean;
      PT    : constant Byte_Array := Str_To_Bytes ("NetizenCore /data encrypted block payload!");
      CT    : Byte_Array (0 .. PT'Length - 1);
      Out_P : Byte_Array (0 .. PT'Length - 1);
      Tag   : Tag_128;
   begin
      Provision_Volume (Str_To_Bytes ("strong-passphrase"), Salt, Nonce, Dek, 10_000, FDE, Meta, Ok);
      Check (Ok and then FDE.Status = Enc_Unlocked, "FDE provision succeeds");

      Lock_Volume (FDE);
      Check (FDE.Status = Enc_Locked and then FDE.Active_Dek = Zero_Key_256, "FDE lock zeroizes DEK");

      Unlock_Volume (FDE, Str_To_Bytes ("wrong-passphrase"), Ok);
      Check (not Ok and then FDE.Status = Enc_Locked, "FDE wrong passphrase stays locked");

      Unlock_Volume (FDE, Str_To_Bytes ("strong-passphrase"), Ok);
      Check (Ok and then FDE.Status = Enc_Unlocked, "FDE right passphrase unlocks");

      Encrypt_Block (FDE, 42, Nonce, PT, CT, Tag, Ok);
      Check (Ok, "FDE block encrypt succeeds");
      Decrypt_Block (FDE, 42, Nonce, CT, Tag, Out_P, Ok);
      Check (Ok and then Out_P = PT, "FDE block decrypt roundtrip");

      --  Wrong block ID (AAD mismatch) must fail!
      Decrypt_Block (FDE, 43, Nonce, CT, Tag, Out_P, Ok);
      Check (not Ok, "FDE block ID AAD mismatch rejected");
   end;

   if Failed = 0 then
      Put_Line ("ALL SPARK SECURITY CORE TESTS PASSED.");
   else
      Put_Line ("FAILED TESTS:" & Natural'Image (Failed));
   end if;
end Test_NC_Security;
