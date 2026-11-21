--  NetizenCore SPARK Ada Security Core — Authoritative Credential & Lock State Machine
--
--  Owns:
--    - PIN / Password format validation
--    - Lock record derivation & constant-time verification (PBKDF2-HMAC-SHA256)
--    - NCLOCKV1 credential sector & NCLOCKMK marker sector serialization + integrity
--    - Fail-closed lock state transitions & progressive retry backoff
--
--  Security invariant (fixes legacy aarch64kernel/src/lock.rs weakness):
--    - Corrupted / destroyed credential metadata (Lock_Destroyed) NEVER unlocks
--      the device; it transitions to a fail-closed locked/destroyed state.

pragma SPARK_Mode (On);

with Interfaces;        use Interfaces;
with NC_Security_Types; use NC_Security_Types;

package NC_Security_Lock
  with Pure
is
   Kind_None     : constant Byte := 0;
   Kind_Pin      : constant Byte := 1;
   Kind_Password : constant Byte := 2;

   Default_Iterations : constant Unsigned_32 := 40_000;
   Min_Iterations     : constant Unsigned_32 := 1;
   Pin_Length         : constant Natural     := 4;
   Password_Min       : constant Natural     := 6;
   Password_Max       : constant Natural     := 32;

   type Lock_Record is record
      Kind       : Byte        := Kind_None;
      Iterations : Unsigned_32 := 0;
      Salt       : Salt_128    := Zero_Salt_128;
      Hash       : Hash_256    := Zero_Hash_256;
   end record;

   type Lock_Marker is record
      Enabled           : Boolean     := False;
      Fails             : Unsigned_32 := 0;
      Credential_Digest : Hash_256    := Zero_Hash_256;
   end record;

   type Lock_Disk_Status is
     (Lock_Disabled,
      Lock_Enabled,
      Lock_Destroyed)
     with Size => 32;

   for Lock_Disk_Status use
     (Lock_Disabled  => 0,
      Lock_Enabled   => 1,
      Lock_Destroyed => 2);

   type Lock_Auth_Result is
     (Auth_Res_Unlocked,
      Auth_Res_Invalid_Secret,
      Auth_Res_Locked_Out,
      Auth_Res_Not_Enabled,
      Auth_Res_Destroyed)
     with Size => 32;

   for Lock_Auth_Result use
     (Auth_Res_Unlocked       => 0,
      Auth_Res_Invalid_Secret => 1,
      Auth_Res_Locked_Out     => 2,
      Auth_Res_Not_Enabled    => 3,
      Auth_Res_Destroyed      => 4);

   type Lock_Machine_State is record
      Status : Lock_Disk_Status := Lock_Disabled;
      Locked : Boolean          := False;
      Rec    : Lock_Record      := (others => <>);
      Fails  : Unsigned_32      := 0;
   end record;

   --  Validate credential syntax (4 ASCII digits for PIN; 6..32 bytes for Password).
   function Secret_Is_Valid
     (Kind   : Byte;
      Secret : Byte_Array) return Boolean
     with Post =>
       (if Kind /= Kind_Pin and then Kind /= Kind_Password then
          not Secret_Is_Valid'Result);

   --  Progressive lockout delay (in seconds) based on consecutive failed attempts.
   function Lockout_Seconds_For (Fails : Unsigned_32) return Unsigned_32
     with Post =>
       (if Fails < 5 then Lockout_Seconds_For'Result = 0
        elsif Fails < 8 then Lockout_Seconds_For'Result = 30
        elsif Fails < 10 then Lockout_Seconds_For'Result = 60
        else Lockout_Seconds_For'Result = 120);

   --  Derive a Lock_Record from a validated secret and caller-supplied random salt.
   procedure Derive_Record
     (Kind       : Byte;
      Secret     : Byte_Array;
      Salt       : Salt_128;
      Iterations : Unsigned_32;
      Rec        : out Lock_Record)
     with Pre => Iterations >= Min_Iterations
                 and then Secret'Length <= Max_Secret_Len;

   --  Verify a secret against a Lock_Record using constant-time comparison.
   procedure Verify_Record
     (Rec    : Lock_Record;
      Secret : Byte_Array;
      Match  : out Boolean)
     with Pre  => Secret'Length <= Max_Secret_Len,
          Post => (if not Secret_Is_Valid (Rec.Kind, Secret)
                      or else Rec.Iterations < Min_Iterations
                   then not Match);

   --  Serialize/Deserialize NCLOCKV1 512-byte credential sector with SHA-256 tail.
   procedure Record_To_Sector
     (Rec    : Lock_Record;
      Sector : out Sector_512);

   procedure Record_From_Sector
     (Sector : Sector_512;
      Rec    : out Lock_Record;
      Valid  : out Boolean);

   --  Serialize/Deserialize NCLOCKMK 512-byte marker sector with SHA-256 tail.
   procedure Marker_To_Sector
     (M      : Lock_Marker;
      Sector : out Sector_512);

   procedure Marker_From_Sector
     (Sector : Sector_512;
      M      : out Lock_Marker;
      Valid  : out Boolean);

   --  Evaluate both persistent sectors and determine authoritative disk lock state.
   procedure Evaluate_Sectors
     (Cred_Sector : Sector_512;
      Mark_Sector : Sector_512;
      Status      : out Lock_Disk_Status;
      Rec         : out Lock_Record;
      Fails       : out Unsigned_32);

   --  Load lock state machine from evaluated disk sectors.
   --  Fail-closed invariant: Lock_Destroyed ALWAYS sets Locked := True.
   procedure Load_Machine
     (Cred_Sector  : Sector_512;
      Mark_Sector  : Sector_512;
      Lock_On_Boot : Boolean;
      Machine      : out Lock_Machine_State)
     with Post =>
       (if Machine.Status = Lock_Destroyed then Machine.Locked)
       and then
       (if Machine.Status = Lock_Enabled and then Lock_On_Boot then Machine.Locked);

   --  Explicitly lock the machine if enabled or destroyed.
   procedure Lock_Now
     (Machine : in out Lock_Machine_State;
      Success : out Boolean)
     with Post =>
       (if Success then Machine.Locked)
       and then (if Machine.Status = Lock_Destroyed then Machine.Locked);

   --  Authenticate a credential attempt against the lock state machine.
   --  Proven invariant: failed authentication or Destroyed state NEVER unlocks.
   procedure Authenticate
     (Machine        : in out Lock_Machine_State;
      Secret         : Byte_Array;
      Lockout_Active : Boolean;
      Result         : out Lock_Auth_Result;
      Delay_Seconds  : out Unsigned_32)
     with Pre  => Secret'Length <= Max_Secret_Len,
          Post =>
            (if Machine.Status = Lock_Destroyed then
               Result = Auth_Res_Destroyed and then Machine.Locked)
            and then
            (if Result = Auth_Res_Invalid_Secret or else Result = Auth_Res_Locked_Out then
               Machine.Locked = Machine'Old.Locked);

end NC_Security_Lock;
