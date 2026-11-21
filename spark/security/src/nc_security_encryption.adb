--  NetizenCore SPARK Ada Security Core — Full-Data Encryption Body

pragma SPARK_Mode (On);

with NC_Security_Crypto; use NC_Security_Crypto;

package body NC_Security_Encryption is

   Enc_Magic : constant Byte_Array (0 .. 7) :=
     (Character'Pos ('N'), Character'Pos ('C'), Character'Pos ('E'), Character'Pos ('N'),
      Character'Pos ('C'), Character'Pos ('R'), Character'Pos ('V'), Character'Pos ('1'));

   Wrap_AAD : constant Byte_Array (0 .. 11) :=
     (Character'Pos ('N'), Character'Pos ('C'), Character'Pos ('-'), Character'Pos ('D'),
      Character'Pos ('A'), Character'Pos ('T'), Character'Pos ('A'), Character'Pos ('-'),
      Character'Pos ('D'), Character'Pos ('E'), Character'Pos ('K'), Character'Pos ('1'));

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

   function Load_LE64 (Src : Byte_Array) return Unsigned_64
     with Pre => Src'Length = 8
   is
      Res : Unsigned_64 := 0;
   begin
      for I in 0 .. 7 loop
         Res := Res or Shift_Left (Unsigned_64 (Src (Src'First + I)), I * 8);
      end loop;
      return Res;
   end Load_LE64;

   procedure Store_LE64 (Val : Unsigned_64; Dst : out Byte_Array)
     with Pre => Dst'Length = 8 and then Dst'First <= Natural'Last - 7
   is
   begin
      Dst := (others => 0);
      for I in 0 .. 7 loop
         Dst (Dst'First + I) := Byte (Shift_Right (Val, I * 8) and 16#FF#);
      end loop;
   end Store_LE64;

   function Is_All_Zero (Buf : Byte_Array) return Boolean is
   begin
      for I in Buf'Range loop
         if Buf (I) /= 0 then
            return False;
         end if;
      end loop;
      return True;
   end Is_All_Zero;

   procedure Metadata_To_Sector
     (Meta   : Encryption_Metadata;
      Sector : out Sector_512)
   is
      Tail : Hash_256;
   begin
      Sector := (others => 0);
      Sector (0 .. 7) := Enc_Magic;
      Sector (8)      := Meta.Version;
      Store_LE32 (Meta.Kdf_Iterations, Sector (12 .. 15));
      Sector (16 .. 31) := Meta.Kdf_Salt;
      Sector (32 .. 43) := Meta.Wrap_Nonce;
      Sector (44 .. 75) := Meta.Wrapped_Dek;
      Sector (76 .. 91) := Meta.Wrap_Tag;
      Store_LE64 (Meta.Metadata_Gen, Sector (92 .. 99));
      Sha256_One_Shot (Sector (0 .. 479), Tail);
      Sector (480 .. 511) := Tail;
   end Metadata_To_Sector;

   procedure Metadata_From_Sector
     (Sector : Sector_512;
      Meta   : out Encryption_Metadata;
      Valid  : out Boolean)
   is
      Tail  : Hash_256;
      Iters : Unsigned_32;
   begin
      Meta  := (others => <>);
      Valid := False;

      if not Constant_Time_Eq (Sector (0 .. 7), Enc_Magic)
        or else Sector (8) /= Enc_Version_1
      then
         return;
      end if;

      Sha256_One_Shot (Sector (0 .. 479), Tail);
      if not Constant_Time_Eq (Tail, Sector (480 .. 511)) then
         return;
      end if;

      Iters := Load_LE32 (Sector (12), Sector (13), Sector (14), Sector (15));
      if not Is_Valid_Kdf_Iterations (Iters) then
         return;
      end if;

      Meta :=
        (Version        => Enc_Version_1,
         Kdf_Iterations => Iters,
         Kdf_Salt       => Sector (16 .. 31),
         Wrap_Nonce     => Sector (32 .. 43),
         Wrapped_Dek    => Sector (44 .. 75),
         Wrap_Tag       => Sector (76 .. 91),
         Metadata_Gen   => Load_LE64 (Sector (92 .. 99)));
      Valid := True;
   end Metadata_From_Sector;

   procedure Provision_Volume
     (Passphrase     : Byte_Array;
      Salt           : Salt_128;
      Wrap_Nonce     : Nonce_96;
      Raw_Dek        : Key_256;
      Kdf_Iterations : Unsigned_32;
      State          : out Encryption_State;
      Meta_Sector    : out Sector_512;
      Success        : out Boolean)
   is
      Kek         : Key_256;
      Wrapped_Dek : Key_256;
      Wrap_Tag    : Tag_128;
   begin
      State :=
        (Status     => Enc_Unconfigured,
         Meta       => (others => <>),
         Active_Dek => Zero_Key_256);
      Meta_Sector := (others => 0);
      Success     := False;

      --  Reject weak passphrases, invalid KDF iterations, or all-zero DEK/Salt.
      if Passphrase'Length < 6
        or else not Is_Valid_Kdf_Iterations (Kdf_Iterations)
        or else Is_All_Zero (Raw_Dek)
        or else Is_All_Zero (Salt)
      then
         return;
      end if;

      Pbkdf2_Hmac_Sha256 (Passphrase, Salt, Kdf_Iterations, Kek);
      ChaCha20_Poly1305_Encrypt
        (Key        => Kek,
         Nonce      => Wrap_Nonce,
         AAD        => Wrap_AAD,
         Plaintext  => Raw_Dek,
         Ciphertext => Wrapped_Dek,
         Tag        => Wrap_Tag);
      Secure_Zero (Kek);

      State.Meta :=
        (Version        => Enc_Version_1,
         Kdf_Iterations => Kdf_Iterations,
         Kdf_Salt       => Salt,
         Wrap_Nonce     => Wrap_Nonce,
         Wrapped_Dek    => Wrapped_Dek,
         Wrap_Tag       => Wrap_Tag,
         Metadata_Gen   => 1);
      State.Active_Dek := Raw_Dek;
      State.Status     := Enc_Unlocked;

      Metadata_To_Sector (State.Meta, Meta_Sector);
      Success := True;
   end Provision_Volume;

   procedure Load_Metadata_Sector
     (Meta_Sector : Sector_512;
      State       : out Encryption_State)
   is
      Meta  : Encryption_Metadata;
      Valid : Boolean;
   begin
      State :=
        (Status     => Enc_Unconfigured,
         Meta       => (others => <>),
         Active_Dek => Zero_Key_256);

      if Is_All_Zero (Meta_Sector) then
         State.Status := Enc_Unconfigured;
         return;
      end if;

      Metadata_From_Sector (Meta_Sector, Meta, Valid);
      if Valid then
         State.Meta   := Meta;
         State.Status := Enc_Locked;
      else
         State.Status := Enc_Corrupted;
      end if;
   end Load_Metadata_Sector;

   procedure Unlock_Volume
     (State      : in out Encryption_State;
      Passphrase : Byte_Array;
      Unlocked   : out Boolean)
   is
      Kek     : Key_256;
      Unwrap  : Key_256;
      Tag_Ok  : Boolean;
   begin
      Unlocked := False;

      if State.Status /= Enc_Locked
        or else Passphrase'Length = 0
        or else not Is_Valid_Kdf_Iterations (State.Meta.Kdf_Iterations)
      then
         Secure_Zero (State.Active_Dek);
         if State.Status = Enc_Unlocked then
            State.Status := Enc_Locked;
         end if;
         return;
      end if;

      Pbkdf2_Hmac_Sha256
        (Passphrase, State.Meta.Kdf_Salt, State.Meta.Kdf_Iterations, Kek);
      ChaCha20_Poly1305_Decrypt
        (Key        => Kek,
         Nonce      => State.Meta.Wrap_Nonce,
         AAD        => Wrap_AAD,
         Ciphertext => State.Meta.Wrapped_Dek,
         Tag        => State.Meta.Wrap_Tag,
         Plaintext  => Unwrap,
         Valid      => Tag_Ok);
      Secure_Zero (Kek);

      if Tag_Ok then
         State.Active_Dek := Unwrap;
         State.Status     := Enc_Unlocked;
         Unlocked         := True;
      else
         Secure_Zero (State.Active_Dek);
         State.Status := Enc_Locked;
         Unlocked     := False;
      end if;
      Secure_Zero (Unwrap);
   end Unlock_Volume;

   procedure Lock_Volume (State : in out Encryption_State) is
   begin
      Secure_Zero (State.Active_Dek);
      if State.Status = Enc_Unlocked then
         State.Status := Enc_Locked;
      end if;
   end Lock_Volume;

   procedure Encrypt_Block
     (State      : Encryption_State;
      Block_Id   : Unsigned_64;
      Nonce      : Nonce_96;
      Plaintext  : Byte_Array;
      Ciphertext : out Byte_Array;
      Tag        : out Tag_128;
      Success    : out Boolean)
   is
      Block_AAD : Byte_Array (0 .. 11) :=
        (Character'Pos ('N'), Character'Pos ('C'), Character'Pos ('B'), Character'Pos ('1'),
         others => 0);
   begin
      Ciphertext := (others => 0);
      Tag        := Zero_Tag_128;
      if State.Status /= Enc_Unlocked then
         Success := False;
         return;
      end if;
      Store_LE64 (Block_Id, Block_AAD (4 .. 11));
      ChaCha20_Poly1305_Encrypt
        (Key        => State.Active_Dek,
         Nonce      => Nonce,
         AAD        => Block_AAD,
         Plaintext  => Plaintext,
         Ciphertext => Ciphertext,
         Tag        => Tag);
      Success := True;
   end Encrypt_Block;

   procedure Decrypt_Block
     (State      : Encryption_State;
      Block_Id   : Unsigned_64;
      Nonce      : Nonce_96;
      Ciphertext : Byte_Array;
      Tag        : Tag_128;
      Plaintext  : out Byte_Array;
      Success    : out Boolean)
   is
      Block_AAD : Byte_Array (0 .. 11) :=
        (Character'Pos ('N'), Character'Pos ('C'), Character'Pos ('B'), Character'Pos ('1'),
         others => 0);
   begin
      if State.Status /= Enc_Unlocked then
         Secure_Zero (Plaintext);
         Success := False;
         return;
      end if;
      Store_LE64 (Block_Id, Block_AAD (4 .. 11));
      ChaCha20_Poly1305_Decrypt
        (Key        => State.Active_Dek,
         Nonce      => Nonce,
         AAD        => Block_AAD,
         Ciphertext => Ciphertext,
         Tag        => Tag,
         Plaintext  => Plaintext,
         Valid      => Success);
   end Decrypt_Block;

end NC_Security_Encryption;
