--  NetizenCore SPARK Ada Security Core — Authoritative Credential & Lock State Machine Body

pragma SPARK_Mode (On);

with NC_Security_Crypto; use NC_Security_Crypto;

package body NC_Security_Lock is

   Cred_Magic : constant Byte_Array (0 .. 7) :=
     (Character'Pos ('N'), Character'Pos ('C'), Character'Pos ('L'), Character'Pos ('O'),
      Character'Pos ('C'), Character'Pos ('K'), Character'Pos ('V'), Character'Pos ('1'));

   Mark_Magic : constant Byte_Array (0 .. 7) :=
     (Character'Pos ('N'), Character'Pos ('C'), Character'Pos ('L'), Character'Pos ('O'),
      Character'Pos ('C'), Character'Pos ('K'), Character'Pos ('M'), Character'Pos ('K'));

   function Load_LE32 (B0, B1, B2, B3 : Byte) return Unsigned_32 is
     (Unsigned_32 (B0)
      or Shift_Left (Unsigned_32 (B1), 8)
      or Shift_Left (Unsigned_32 (B2), 16)
      or Shift_Left (Unsigned_32 (B3), 24));

   procedure Store_LE32 (Val : Unsigned_32; Dst : out Byte_Array)
     with Pre => Dst'Length = 4 and then Dst'First <= Natural'Last - 3
   is
   begin
      Dst := (others => 0);
      Dst (Dst'First)     := Byte (Val and 16#FF#);
      Dst (Dst'First + 1) := Byte (Shift_Right (Val, 8) and 16#FF#);
      Dst (Dst'First + 2) := Byte (Shift_Right (Val, 16) and 16#FF#);
      Dst (Dst'First + 3) := Byte (Shift_Right (Val, 24) and 16#FF#);
   end Store_LE32;

   function Is_All_Zero (Sector : Sector_512) return Boolean is
   begin
      for I in Sector'Range loop
         if Sector (I) /= 0 then
            return False;
         end if;
      end loop;
      return True;
   end Is_All_Zero;

   function Secret_Is_Valid
     (Kind   : Byte;
      Secret : Byte_Array) return Boolean
   is
   begin
      if Secret'Last < Secret'First then
         return False;
      end if;
      if Kind = Kind_Pin then
         if Secret'Last - Secret'First /= Pin_Length - 1 then
            return False;
         end if;
         for I in Secret'Range loop
            if Secret (I) < Character'Pos ('0')
              or else Secret (I) > Character'Pos ('9')
            then
               return False;
            end if;
         end loop;
         return True;
      elsif Kind = Kind_Password then
         return (Secret'Last - Secret'First) >= Password_Min - 1
           and then (Secret'Last - Secret'First) <= Password_Max - 1;
      else
         return False;
      end if;
   end Secret_Is_Valid;

   function Lockout_Seconds_For (Fails : Unsigned_32) return Unsigned_32 is
   begin
      if Fails >= 10 then
         return 120;
      elsif Fails >= 8 then
         return 60;
      elsif Fails >= 5 then
         return 30;
      else
         return 0;
      end if;
   end Lockout_Seconds_For;

   procedure Derive_Record
     (Kind       : Byte;
      Secret     : Byte_Array;
      Salt       : Salt_128;
      Iterations : Unsigned_32;
      Rec        : out Lock_Record)
   is
      Derived : Hash_256;
   begin
      Pbkdf2_Hmac_Sha256 (Secret, Salt, Iterations, Derived);
      Rec :=
        (Kind       => Kind,
         Iterations => Iterations,
         Salt       => Salt,
         Hash       => Derived);
   end Derive_Record;

   procedure Verify_Record
     (Rec    : Lock_Record;
      Secret : Byte_Array;
      Match  : out Boolean)
   is
      Derived : Hash_256;
   begin
      if not Secret_Is_Valid (Rec.Kind, Secret)
        or else Rec.Iterations < Min_Iterations
      then
         Match := False;
         return;
      end if;
      Pbkdf2_Hmac_Sha256 (Secret, Rec.Salt, Rec.Iterations, Derived);
      Match := Constant_Time_Eq (Derived, Rec.Hash);
      Secure_Zero (Derived);
   end Verify_Record;

   procedure Record_To_Sector
     (Rec    : Lock_Record;
      Sector : out Sector_512)
   is
      Tail : Hash_256;
   begin
      Sector := (others => 0);
      Sector (0 .. 7) := Cred_Magic;
      Sector (8)      := 1;
      Sector (9)      := Rec.Kind;
      Store_LE32 (Rec.Iterations, Sector (12 .. 15));
      Sector (16 .. 31) := Rec.Salt;
      Sector (32 .. 63) := Rec.Hash;
      Sha256_One_Shot (Sector (0 .. 479), Tail);
      Sector (480 .. 511) := Tail;
   end Record_To_Sector;

   procedure Record_From_Sector
     (Sector : Sector_512;
      Rec    : out Lock_Record;
      Valid  : out Boolean)
   is
      Tail  : Hash_256;
      Kind  : Byte;
      Iters : Unsigned_32;
   begin
      Rec   := (others => <>);
      Valid := False;

      if not Constant_Time_Eq (Sector (0 .. 7), Cred_Magic)
        or else Sector (8) /= 1
      then
         return;
      end if;

      Sha256_One_Shot (Sector (0 .. 479), Tail);
      if not Constant_Time_Eq (Tail, Sector (480 .. 511)) then
         return;
      end if;

      Kind := Sector (9);
      if Kind /= Kind_Pin and then Kind /= Kind_Password then
         return;
      end if;

      Iters := Load_LE32 (Sector (12), Sector (13), Sector (14), Sector (15));
      if Iters < Min_Iterations then
         return;
      end if;

      Rec :=
        (Kind       => Kind,
         Iterations => Iters,
         Salt       => Sector (16 .. 31),
         Hash       => Sector (32 .. 63));
      Valid := True;
   end Record_From_Sector;

   procedure Marker_To_Sector
     (M      : Lock_Marker;
      Sector : out Sector_512)
   is
      Tail : Hash_256;
   begin
      Sector := (others => 0);
      Sector (0 .. 7) := Mark_Magic;
      Sector (8)      := 1;
      Sector (9)      := (if M.Enabled then 1 else 0);
      Store_LE32 (M.Fails, Sector (12 .. 15));
      Sector (16 .. 47) := M.Credential_Digest;
      Sha256_One_Shot (Sector (0 .. 479), Tail);
      Sector (480 .. 511) := Tail;
   end Marker_To_Sector;

   procedure Marker_From_Sector
     (Sector : Sector_512;
      M      : out Lock_Marker;
      Valid  : out Boolean)
   is
      Tail : Hash_256;
   begin
      M     := (others => <>);
      Valid := False;

      if not Constant_Time_Eq (Sector (0 .. 7), Mark_Magic)
        or else Sector (8) /= 1
      then
         return;
      end if;

      Sha256_One_Shot (Sector (0 .. 479), Tail);
      if not Constant_Time_Eq (Tail, Sector (480 .. 511)) then
         return;
      end if;

      if Sector (9) /= 0 and then Sector (9) /= 1 then
         return;
      end if;

      M :=
        (Enabled           => Sector (9) = 1,
         Fails             => Load_LE32 (Sector (12), Sector (13), Sector (14), Sector (15)),
         Credential_Digest => Sector (16 .. 47));
      Valid := True;
   end Marker_From_Sector;

   procedure Evaluate_Sectors
     (Cred_Sector : Sector_512;
      Mark_Sector : Sector_512;
      Status      : out Lock_Disk_Status;
      Rec         : out Lock_Record;
      Fails       : out Unsigned_32)
   is
      Cred_Blank  : constant Boolean := Is_All_Zero (Cred_Sector);
      Mark_Blank  : constant Boolean := Is_All_Zero (Mark_Sector);
      M           : Lock_Marker;
      M_Valid     : Boolean;
      R_Valid     : Boolean;
      Cred_Digest : Hash_256;
   begin
      Rec   := (others => <>);
      Fails := 0;

      if Cred_Blank and then Mark_Blank then
         Status := Lock_Disabled;
         return;
      end if;

      Marker_From_Sector (Mark_Sector, M, M_Valid);
      if not M_Valid then
         Status := Lock_Destroyed;
         return;
      end if;

      if not M.Enabled then
         if Cred_Blank then
            Status := Lock_Disabled;
         else
            Status := Lock_Destroyed;
         end if;
         return;
      end if;

      Record_From_Sector (Cred_Sector, Rec, R_Valid);
      if not R_Valid then
         Status := Lock_Destroyed;
         return;
      end if;

      Sha256_One_Shot (Cred_Sector, Cred_Digest);
      if not Constant_Time_Eq (Cred_Digest, M.Credential_Digest) then
         Rec    := (others => <>);
         Status := Lock_Destroyed;
         return;
      end if;

      Fails  := M.Fails;
      Status := Lock_Enabled;
   end Evaluate_Sectors;

   procedure Load_Machine
     (Cred_Sector  : Sector_512;
      Mark_Sector  : Sector_512;
      Lock_On_Boot : Boolean;
      Machine      : out Lock_Machine_State)
   is
      Status : Lock_Disk_Status;
      Rec    : Lock_Record;
      Fails  : Unsigned_32;
   begin
      Evaluate_Sectors (Cred_Sector, Mark_Sector, Status, Rec, Fails);
      case Status is
         when Lock_Disabled =>
            Machine :=
              (Status => Lock_Disabled,
               Locked => False,
               Rec    => (others => <>),
               Fails  => 0);
         when Lock_Enabled =>
            Machine :=
              (Status => Lock_Enabled,
               Locked => Lock_On_Boot,
               Rec    => Rec,
               Fails  => Fails);
         when Lock_Destroyed =>
            --  FAIL-CLOSED: corrupted credential state NEVER becomes unlocked!
            Machine :=
              (Status => Lock_Destroyed,
               Locked => True,
               Rec    => (others => <>),
               Fails  => 0);
      end case;
   end Load_Machine;

   procedure Lock_Now
     (Machine : in out Lock_Machine_State;
      Success : out Boolean)
   is
   begin
      case Machine.Status is
         when Lock_Enabled =>
            Machine.Locked := True;
            Success := True;
         when Lock_Destroyed =>
            Machine.Locked := True;
            Success := True;
         when Lock_Disabled =>
            Success := False;
      end case;
   end Lock_Now;

   procedure Authenticate
     (Machine        : in out Lock_Machine_State;
      Secret         : Byte_Array;
      Lockout_Active : Boolean;
      Result         : out Lock_Auth_Result;
      Delay_Seconds  : out Unsigned_32)
   is
      Match : Boolean;
   begin
      Delay_Seconds := 0;

      if Machine.Status = Lock_Destroyed then
         Machine.Locked := True;
         Result := Auth_Res_Destroyed;
         return;
      end if;

      if Machine.Status = Lock_Disabled then
         Machine.Locked := False;
         Result := Auth_Res_Not_Enabled;
         return;
      end if;

      if Lockout_Active then
         Result := Auth_Res_Locked_Out;
         return;
      end if;

      Verify_Record (Machine.Rec, Secret, Match);
      if Match then
         Machine.Locked := False;
         Machine.Fails  := 0;
         Result         := Auth_Res_Unlocked;
      else
         if Machine.Fails < Unsigned_32'Last then
            Machine.Fails := Machine.Fails + 1;
         end if;
         Delay_Seconds := Lockout_Seconds_For (Machine.Fails);
         if Delay_Seconds > 0 then
            Result := Auth_Res_Locked_Out;
         else
            Result := Auth_Res_Invalid_Secret;
         end if;
      end if;
   end Authenticate;

end NC_Security_Lock;
