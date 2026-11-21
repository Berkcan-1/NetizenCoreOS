--  NetizenCore SPARK Ada Security Core — Authoritative Capabilities
--  Single source of truth for all capability bit assignments, masks,
--  user/system ceilings, and validation rules.
--
--  Compatibility: bits 0..21 match legacy nc-caps u32 values identically.
--  Representation is 64-bit (Cap_Mask) for future mobile permission expansion,
--  with explicit u32 compatibility helpers.

pragma SPARK_Mode (On);

with Interfaces; use Interfaces;

package NC_Security_Capabilities
  with Pure
is
   subtype Cap_Mask is Unsigned_64;

   Cap_None                   : constant Cap_Mask := 0;
   Cap_Settings_Read          : constant Cap_Mask := 2 ** 0;
   Cap_Settings_Write_Display : constant Cap_Mask := 2 ** 1;
   Cap_Settings_Write_Locale  : constant Cap_Mask := 2 ** 2;
   Cap_Settings_Write_Keybd   : constant Cap_Mask := 2 ** 3;
   Cap_Settings_Write_Network : constant Cap_Mask := 2 ** 4;
   Cap_Settings_Write_Drivers : constant Cap_Mask := 2 ** 5;
   Cap_Fs_User_Read           : constant Cap_Mask := 2 ** 6;
   Cap_Fs_User_Write          : constant Cap_Mask := 2 ** 7;
   Cap_Fs_User_Delete         : constant Cap_Mask := 2 ** 8;
   Cap_Fs_Grant_Read          : constant Cap_Mask := 2 ** 9;
   Cap_Fs_Grant_Write         : constant Cap_Mask := 2 ** 10;
   Cap_Package_Install        : constant Cap_Mask := 2 ** 11;
   Cap_Network_Client         : constant Cap_Mask := 2 ** 12;
   Cap_System_Audit_Read      : constant Cap_Mask := 2 ** 13;
   Cap_Ipc_Endpoint           : constant Cap_Mask := 2 ** 14;
   Cap_Audio_Play             : constant Cap_Mask := 2 ** 15;
   Cap_Notify_Post            : constant Cap_Mask := 2 ** 16;
   Cap_Fs_All_Read            : constant Cap_Mask := 2 ** 17;
   Cap_Fs_All_Write           : constant Cap_Mask := 2 ** 18;
   Cap_Process_Launch         : constant Cap_Mask := 2 ** 19;
   Cap_Package_Remove         : constant Cap_Mask := 2 ** 20;
   Cap_Security_Manage        : constant Cap_Mask := 2 ** 21;

   --  Bitmask of all 22 currently defined capabilities (bits 0..21).
   All_Known_Caps : constant Cap_Mask := 16#003F_FFFF#;

   --  Kernel internal full mask (never grantable to user-space apps).
   Cap_Kernel_Internal : constant Cap_Mask := 16#FFFF_FFFF#;

   --  Maximum capabilities a User-tier application may ever hold.
   User_Allowed_Mask : constant Cap_Mask :=
     Cap_Settings_Read
     or Cap_Fs_User_Read
     or Cap_Fs_User_Write
     or Cap_Fs_User_Delete
     or Cap_Network_Client
     or Cap_Ipc_Endpoint
     or Cap_Audio_Play
     or Cap_Notify_Post;

   --  System-only capabilities: disjoint from User_Allowed_Mask.
   System_Only_Mask : constant Cap_Mask :=
     All_Known_Caps and (not User_Allowed_Mask);

   --  Third-party verified package ceiling (strictly within User_Allowed_Mask).
   Third_Party_Verified_Ceiling : constant Cap_Mask :=
     Cap_Settings_Read
     or Cap_Fs_User_Read
     or Cap_Fs_User_Write
     or Cap_Network_Client
     or Cap_Ipc_Endpoint
     or Cap_Audio_Play
     or Cap_Notify_Post;

   --  Check whether a mask contains only known capability bits.
   function Is_Valid_Mask (Mask : Cap_Mask) return Boolean is
     ((Mask and (not All_Known_Caps)) = 0);

   --  Check whether a value is a single known capability bit.
   function Is_Single_Known_Cap (Cap : Cap_Mask) return Boolean is
     (Cap /= 0
      and then Is_Valid_Mask (Cap)
      and then (Cap and (Cap - 1)) = 0);

   --  Clamp declared capabilities to the User-tier ceiling.
   --  Proven invariant: result NEVER contains any System_Only or unknown bit.
   function Clamp_User (Declared : Cap_Mask) return Cap_Mask
     with Post =>
       (Clamp_User'Result and System_Only_Mask) = 0
       and then Is_Valid_Mask (Clamp_User'Result)
       and then (Clamp_User'Result and (not Declared)) = 0;

   --  Return bits dropped when clamping for User-tier.
   function Dropped_For_User (Declared : Cap_Mask) return Cap_Mask
     with Post =>
       (Dropped_For_User'Result and User_Allowed_Mask) = 0;

   --  Combine requested, user-approved, and policy-ceiling masks.
   --  A capability is granted ONLY if requested AND approved AND within ceiling
   --  AND known.
   function Compute_Effective_Grant
     (Requested : Cap_Mask;
      Approved  : Cap_Mask;
      Ceiling   : Cap_Mask) return Cap_Mask
     with Post =>
       Is_Valid_Mask (Compute_Effective_Grant'Result)
       and then (Compute_Effective_Grant'Result and (not Requested)) = 0
       and then (Compute_Effective_Grant'Result and (not Approved)) = 0
       and then (Compute_Effective_Grant'Result and (not Ceiling)) = 0;

   --  Return True iff Mask grants single capability Cap.
   function Has_Capability (Mask : Cap_Mask; Cap : Cap_Mask) return Boolean
     with Post =>
       (if not Is_Single_Known_Cap (Cap) then not Has_Capability'Result);

end NC_Security_Capabilities;
