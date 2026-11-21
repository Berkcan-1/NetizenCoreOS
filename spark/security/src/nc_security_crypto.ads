--  NetizenCore SPARK Ada Security Core — Authoritative Cryptographic Primitives
--
--  Implements established, standardized cryptographic algorithms in SPARK Ada:
--    - SHA-256 (FIPS 180-4) — streaming and one-shot
--    - HMAC-SHA256 (RFC 2104)
--    - PBKDF2-HMAC-SHA256 (RFC 2898 / NIST SP 800-132)
--    - Constant-time byte comparison
--    - Secure memory zeroization
--    - ChaCha20-Poly1305 AEAD (RFC 8439) for authenticated data/key encryption
--
--  Assurance separation:
--    - Functional safety (no overflow, no out-of-bounds, full initialization)
--      is formally verified by GNATprove.
--    - Algorithm correctness is verified against NIST/RFC known-answer test vectors.
--    - Constant-time execution and hardware entropy quality are distinct properties
--      documented separately in the security architecture report.

pragma SPARK_Mode (On);

with Interfaces;        use Interfaces;
with NC_Security_Types; use NC_Security_Types;

package NC_Security_Crypto
  with Pure
is
   type Word32_Array is array (Natural range <>) of Unsigned_32;

   type Sha256_Context is record
      State  : Word32_Array (0 .. 7) := (others => 0);
      Buffer : Byte_Array (0 .. 63)  := (others => 0);
      Filled : Natural               := 0;
      Total  : Unsigned_64           := 0;
   end record;

   --  Constant-time equality for equal-length byte slices.
   function Constant_Time_Eq (A, B : Byte_Array) return Boolean
     with Post =>
       Constant_Time_Eq'Result =
         (A'Length = B'Length and then
          (for all I in 0 .. A'Length - 1 =>
             A (A'First + I) = B (B'First + I)));

   --  Explicit memory clearing for secret material.
   procedure Secure_Zero (Data : out Byte_Array)
     with Post => (for all I in Data'Range => Data (I) = 0);

   --  Streaming SHA-256 (FIPS 180-4).
   procedure Sha256_Init (Ctx : out Sha256_Context)
     with Post => Ctx.Filled = 0 and then Ctx.Total = 0;

   procedure Sha256_Update
     (Ctx  : in out Sha256_Context;
      Data : Byte_Array)
     with Pre  => Ctx.Filled < 64,
          Post => Ctx.Filled < 64;

   procedure Sha256_Finalize
     (Ctx    : in out Sha256_Context;
      Digest : out Hash_256)
     with Pre => Ctx.Filled < 64;

   procedure Sha256_One_Shot
     (Data   : Byte_Array;
      Digest : out Hash_256);

   --  HMAC-SHA256 (RFC 2104).
   procedure Hmac_Sha256
     (Key     : Byte_Array;
      Message : Byte_Array;
      Mac     : out Hash_256);

   --  PBKDF2-HMAC-SHA256 (RFC 2898).
   procedure Pbkdf2_Hmac_Sha256
     (Password   : Byte_Array;
      Salt       : Byte_Array;
      Iterations : Unsigned_32;
      Output     : out Byte_Array)
     with Pre => Iterations >= 1
                 and then Output'Length >= 1
                 and then Output'Length <= 1024;

   --  ChaCha20-Poly1305 AEAD (RFC 8439) — Encrypt & Authenticate.
   procedure ChaCha20_Poly1305_Encrypt
     (Key        : Key_256;
      Nonce      : Nonce_96;
      AAD        : Byte_Array;
      Plaintext  : Byte_Array;
      Ciphertext : out Byte_Array;
      Tag        : out Tag_128)
     with Pre => Ciphertext'Length = Plaintext'Length
                 and then Plaintext'Length <= 65536
                 and then AAD'Length <= 4096;

   --  ChaCha20-Poly1305 AEAD (RFC 8439) — Verify Tag & Decrypt.
   --  Fail-closed: if Valid = False, Plaintext is zeroized.
   procedure ChaCha20_Poly1305_Decrypt
     (Key        : Key_256;
      Nonce      : Nonce_96;
      AAD        : Byte_Array;
      Ciphertext : Byte_Array;
      Tag        : Tag_128;
      Plaintext  : out Byte_Array;
      Valid      : out Boolean)
     with Pre  => Plaintext'Length = Ciphertext'Length
                  and then Ciphertext'Length <= 65536
                  and then AAD'Length <= 4096,
          Post => (if not Valid then
                     (for all I in Plaintext'Range => Plaintext (I) = 0));

end NC_Security_Crypto;
