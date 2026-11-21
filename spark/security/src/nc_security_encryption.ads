--  NetizenCore SPARK Ada Security Core — Full-Data Encryption (/data) Key & State Management
--
--  Owns:
--    - Encryption metadata sector (NCENCRV1) validation and integrity verification
--    - KDF parameter validation (PBKDF2-HMAC-SHA256, minimum iteration floor)
--    - Key hierarchy: KEK derived from user passphrase + salt wraps 256-bit DEK
--      using RFC 8439 ChaCha20-Poly1305 AEAD (authenticated key wrapping)
--    - Fail-closed unlock/lock state machine:
--        * Corrupted metadata is ALWAYS rejected.
--        * Failed authentication NEVER unlocks /data.
--        * Locking immediately zeroizes the in-memory DEK.
--    - Authenticated block-level encryption/decryption (ChaCha20-Poly1305 with
--      sector-index AAD + unique nonce per sector write).
--
--  Note on random-access block encryption & entropy:
--    - Nonce uniqueness and hardware CSPRNG quality are enforced at the trusted
--      boundary; callers MUST supply hardware CSPRNG bytes for salts, DEKs, and
--      nonces. Predictable timer/RTC values MUST NOT be used for key generation.

pragma SPARK_Mode (On);

with Interfaces;        use Interfaces;
with NC_Security_Types; use NC_Security_Types;

package NC_Security_Encryption
  with Pure
is
   Enc_Version_1     : constant Byte        := 1;
   Min_Kdf_Iters     : constant Unsigned_32 := 10_000;
   Default_Kdf_Iters : constant Unsigned_32 := 40_000;
   Max_Kdf_Iters     : constant Unsigned_32 := 1_000_000;

   type Encryption_Status is
     (Enc_Unconfigured,
      Enc_Locked,
      Enc_Unlocked,
      Enc_Corrupted)
     with Size => 32;

   for Encryption_Status use
     (Enc_Unconfigured => 0,
      Enc_Locked       => 1,
      Enc_Unlocked     => 2,
      Enc_Corrupted    => 3);

   type Encryption_Metadata is record
      Version        : Byte        := 0;
      Kdf_Iterations : Unsigned_32 := 0;
      Kdf_Salt       : Salt_128    := Zero_Salt_128;
      Wrap_Nonce     : Nonce_96    := (others => 0);
      Wrapped_Dek    : Key_256     := Zero_Key_256;
      Wrap_Tag       : Tag_128     := Zero_Tag_128;
      Metadata_Gen   : Unsigned_64 := 0;
   end record;

   type Encryption_State is record
      Status     : Encryption_Status   := Enc_Unconfigured;
      Meta       : Encryption_Metadata := (others => <>);
      Active_Dek : Key_256             := Zero_Key_256;
   end record;

   function Is_Valid_Kdf_Iterations (Iters : Unsigned_32) return Boolean is
     (Iters >= Min_Kdf_Iters and then Iters <= Max_Kdf_Iters);

   --  Serialize / Deserialize NCENCRV1 512-byte metadata sector with SHA-256 tail.
   procedure Metadata_To_Sector
     (Meta   : Encryption_Metadata;
      Sector : out Sector_512)
     with Pre => Meta.Version = Enc_Version_1
                 and then Is_Valid_Kdf_Iterations (Meta.Kdf_Iterations);

   procedure Metadata_From_Sector
     (Sector : Sector_512;
      Meta   : out Encryption_Metadata;
      Valid  : out Boolean)
     with Post =>
       (if Valid then
          Meta.Version = Enc_Version_1
          and then Is_Valid_Kdf_Iterations (Meta.Kdf_Iterations));

   --  Initialize new encrypted volume metadata by wrapping a caller-supplied
   --  hardware-random DEK under a KEK derived from Passphrase + Salt.
   procedure Provision_Volume
     (Passphrase     : Byte_Array;
      Salt           : Salt_128;
      Wrap_Nonce     : Nonce_96;
      Raw_Dek        : Key_256;
      Kdf_Iterations : Unsigned_32;
      State          : out Encryption_State;
      Meta_Sector    : out Sector_512;
      Success        : out Boolean)
     with Pre  => Passphrase'Length <= Max_Secret_Len,
          Post =>
            (if not Success then
               State.Status = Enc_Unconfigured
               and then State.Active_Dek = Zero_Key_256)
            and then
            (if Success then
               State.Status = Enc_Unlocked);

   --  Load encryption state from a persisted metadata sector (starts LOCKED).
   procedure Load_Metadata_Sector
     (Meta_Sector : Sector_512;
      State       : out Encryption_State)
     with Post =>
       State.Active_Dek = Zero_Key_256
       and then State.Status /= Enc_Unlocked;

   --  Authenticate passphrase and unwrap DEK to unlock /data.
   --  Proven invariant: unsuccessful unlock ALWAYS leaves State /= Enc_Unlocked
   --  and Active_Dek zeroized.
   procedure Unlock_Volume
     (State      : in out Encryption_State;
      Passphrase : Byte_Array;
      Unlocked   : out Boolean)
     with Pre  => Passphrase'Length <= Max_Secret_Len,
          Post =>
            (if not Unlocked then
               State.Status /= Enc_Unlocked
               and then State.Active_Dek = Zero_Key_256)
            and then
            (if Unlocked then
               State.Status = Enc_Unlocked);

   --  Lock volume and immediately zeroize in-memory DEK.
   procedure Lock_Volume (State : in out Encryption_State)
     with Post =>
       State.Active_Dek = Zero_Key_256
       and then State.Status /= Enc_Unlocked;

   --  Encrypt a data block using Active_Dek (only permitted when Enc_Unlocked).
   procedure Encrypt_Block
     (State      : Encryption_State;
      Block_Id   : Unsigned_64;
      Nonce      : Nonce_96;
      Plaintext  : Byte_Array;
      Ciphertext : out Byte_Array;
      Tag        : out Tag_128;
      Success    : out Boolean)
     with Pre  => Ciphertext'Length = Plaintext'Length
                  and then Plaintext'Length <= 4096,
          Post => (if State.Status /= Enc_Unlocked then not Success);

   --  Decrypt and authenticate a data block using Active_Dek.
   procedure Decrypt_Block
     (State      : Encryption_State;
      Block_Id   : Unsigned_64;
      Nonce      : Nonce_96;
      Ciphertext : Byte_Array;
      Tag        : Tag_128;
      Plaintext  : out Byte_Array;
      Success    : out Boolean)
     with Pre  => Plaintext'Length = Ciphertext'Length
                  and then Ciphertext'Length <= 4096,
          Post =>
            (if State.Status /= Enc_Unlocked then not Success)
            and then
            (if not Success then
               (for all I in Plaintext'Range => Plaintext (I) = 0));

end NC_Security_Encryption;
