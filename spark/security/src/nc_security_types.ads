--  NetizenCore SPARK Ada Security Core — Authoritative Security Types
--  Single source of truth for bounded security types, ABI constants, and status codes.

pragma SPARK_Mode (On);

with Interfaces; use Interfaces;

package NC_Security_Types
  with Pure
is
   --  Narrow, versioned C/Rust/SPARK ABI version.
   ABI_Version : constant Unsigned_32 := 1;

   --  Bounded sizes (no heap allocation, predictable stack and static footprint).
   Max_App_Id_Len     : constant := 32;
   Max_Publisher_Len  : constant := 64;
   Max_Path_Len       : constant := 256;
   Max_Secret_Len     : constant := 64;
   Max_Path_Depth     : constant := 64;
   Max_Registered_Apps : constant := 32;
   Audit_Ring_Capacity : constant := 64;

   subtype Byte is Unsigned_8;
   type Byte_Array is array (Natural range <>) of Byte;

   subtype Hash_256  is Byte_Array (0 .. 31);
   subtype Key_256   is Byte_Array (0 .. 31);
   subtype Nonce_96  is Byte_Array (0 .. 11);
   subtype Tag_128   is Byte_Array (0 .. 15);
   subtype Salt_128  is Byte_Array (0 .. 15);
   subtype Sector_512 is Byte_Array (0 .. 511);

   Zero_Hash_256 : constant Hash_256 := (others => 0);
   Zero_Key_256  : constant Key_256  := (others => 0);
   Zero_Salt_128 : constant Salt_128 := (others => 0);
   Zero_Tag_128  : constant Tag_128  := (others => 0);

   --  Bounded byte-buffer representations for App IDs and Publishers.
   subtype App_Id_Length is Natural range 0 .. Max_App_Id_Len;
   subtype App_Id_Buf    is Byte_Array (1 .. Max_App_Id_Len);

   type Bounded_App_Id is record
      Len  : App_Id_Length := 0;
      Data : App_Id_Buf    := (others => 0);
   end record;

   Empty_App_Id : constant Bounded_App_Id := (Len => 0, Data => (others => 0));

   subtype Publisher_Length is Natural range 0 .. Max_Publisher_Len;
   subtype Publisher_Buf    is Byte_Array (1 .. Max_Publisher_Len);

   type Bounded_Publisher is record
      Len  : Publisher_Length := 0;
      Data : Publisher_Buf    := (others => 0);
   end record;

   Empty_Publisher : constant Bounded_Publisher := (Len => 0, Data => (others => 0));

   --  Audit tag (24 bytes, compatible with existing AuditEvent.app_id_tag).
   subtype Audit_Tag_Buf is Byte_Array (0 .. 23);
   Zero_Audit_Tag : constant Audit_Tag_Buf := (others => 0);

   --  Deterministic authorization decision codes (fail-closed: non-zero = DENY).
   type Auth_Decision is
     (Auth_Allow,
      Auth_Deny_Unknown_App,
      Auth_Deny_Revoked_App,
      Auth_Deny_Rollback_Version,
      Auth_Deny_Capability_Not_Granted,
      Auth_Deny_Policy_Locked,
      Auth_Deny_Invalid_Input,
      Auth_Deny_Impersonation,
      Auth_Deny_Table_Full,
      Auth_Deny_System_Only,
      Auth_Deny_Untrusted_Publisher)
     with Size => 32;

   for Auth_Decision use
     (Auth_Allow                       => 0,
      Auth_Deny_Unknown_App            => 1,
      Auth_Deny_Revoked_App            => 2,
      Auth_Deny_Rollback_Version       => 3,
      Auth_Deny_Capability_Not_Granted => 4,
      Auth_Deny_Policy_Locked          => 5,
      Auth_Deny_Invalid_Input          => 6,
      Auth_Deny_Impersonation          => 7,
      Auth_Deny_Table_Full             => 8,
      Auth_Deny_System_Only            => 9,
      Auth_Deny_Untrusted_Publisher    => 10);

   --  Filesystem operations and decisions.
   type Fs_Op is (Fs_Op_Read, Fs_Op_Write, Fs_Op_Delete)
     with Size => 32;

   for Fs_Op use
     (Fs_Op_Read   => 0,
      Fs_Op_Write  => 1,
      Fs_Op_Delete => 2);

   type Fs_Decision is
     (Fs_Allow,
      Fs_Deny_Bad_Path,
      Fs_Deny_Read_Only,
      Fs_Deny_Permission)
     with Size => 32;

   for Fs_Decision use
     (Fs_Allow           => 0,
      Fs_Deny_Bad_Path   => 1,
      Fs_Deny_Read_Only  => 2,
      Fs_Deny_Permission => 3);

   --  Trust tiers (User vs System).
   type Trust_Tier is (Tier_User, Tier_System)
     with Size => 32;

   for Trust_Tier use
     (Tier_User   => 0,
      Tier_System => 1);

   --  Package installation source.
   type Install_Source is (Source_User_Data, Source_System_Image)
     with Size => 32;

   for Install_Source use
     (Source_User_Data    => 0,
      Source_System_Image => 1);

   --  Helper: compare two Bounded_App_Ids for exact equality.
   function Equal_App_Id (A, B : Bounded_App_Id) return Boolean
     with Post => Equal_App_Id'Result =
       (A.Len = B.Len and then
        (for all I in 1 .. A.Len => A.Data (I) = B.Data (I)));

   --  Helper: validate App ID syntax (non-empty, safe ASCII identifier chars only,
   --  no path separators, no NUL, no ".." traversal).
   function Is_Valid_App_Id (Id : Bounded_App_Id) return Boolean
     with Post =>
       (if Is_Valid_App_Id'Result then
          Id.Len >= 1 and then Id.Len <= Max_App_Id_Len and then
          (for all I in 1 .. Id.Len =>
             Id.Data (I) /= 0 and then
             Id.Data (I) /= Character'Pos ('/') and then
             Id.Data (I) /= Character'Pos ('\')));

   --  Helper: build a Bounded_App_Id from a fixed String literal (for built-ins).
   function Make_App_Id (S : String) return Bounded_App_Id
     with Pre  => S'Length <= Max_App_Id_Len,
          Post => Make_App_Id'Result.Len = S'Length;

   --  Helper: build a Bounded_Publisher from a String literal.
   function Make_Publisher (S : String) return Bounded_Publisher
     with Pre  => S'Length <= Max_Publisher_Len,
          Post => Make_Publisher'Result.Len = S'Length;

end NC_Security_Types;
