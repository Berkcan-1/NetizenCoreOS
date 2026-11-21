--  NetizenCore SPARK Ada Security Core — Authoritative Cryptographic Primitives Body

pragma SPARK_Mode (On);

package body NC_Security_Crypto is

   --  FIPS 180-4 SHA-256 Round Constants
   Sha256_K : constant Word32_Array (0 .. 63) :=
     (16#428a2f98#, 16#71374491#, 16#b5c0fbcf#, 16#e9b5dba5#,
      16#3956c25b#, 16#59f111f1#, 16#923f82a4#, 16#ab1c5ed5#,
      16#d807aa98#, 16#12835b01#, 16#243185be#, 16#550c7dc3#,
      16#72be5d74#, 16#80deb1fe#, 16#9bdc06a7#, 16#c19bf174#,
      16#e49b69c1#, 16#efbe4786#, 16#0fc19dc6#, 16#240ca1cc#,
      16#2de92c6f#, 16#4a7484aa#, 16#5cb0a9dc#, 16#76f988da#,
      16#983e5152#, 16#a831c66d#, 16#b00327c8#, 16#bf597fc7#,
      16#c6e00bf3#, 16#d5a79147#, 16#06ca6351#, 16#14292967#,
      16#27b70a85#, 16#2e1b2138#, 16#4d2c6dfc#, 16#53380d13#,
      16#650a7354#, 16#766a0abb#, 16#81c2c92e#, 16#92722c85#,
      16#a2bfe8a1#, 16#a81a664b#, 16#c24b8b70#, 16#c76c51a3#,
      16#d192e819#, 16#d6990624#, 16#f40e3585#, 16#106aa070#,
      16#19a4c116#, 16#1e376c08#, 16#2748774c#, 16#34b0bcb5#,
      16#391c0cb3#, 16#4ed8aa4a#, 16#5b9cca4f#, 16#682e6ff3#,
      16#748f82ee#, 16#78a5636f#, 16#84c87814#, 16#8cc70208#,
      16#90befffa#, 16#a4506ceb#, 16#bef9a3f7#, 16#c67178f2#);

   Sha256_H0 : constant Word32_Array (0 .. 7) :=
     (16#6a09e667#, 16#bb67ae85#, 16#3c6ef372#, 16#a54ff53a#,
      16#510e527f#, 16#9b05688c#, 16#1f83d9ab#, 16#5be0cd19#);

   function Load_BE32 (B0, B1, B2, B3 : Byte) return Unsigned_32 is
     (Shift_Left (Unsigned_32 (B0), 24)
      or Shift_Left (Unsigned_32 (B1), 16)
      or Shift_Left (Unsigned_32 (B2), 8)
      or Unsigned_32 (B3));

   function Load_LE32 (B0, B1, B2, B3 : Byte) return Unsigned_32 is
     (Unsigned_32 (B0)
      or Shift_Left (Unsigned_32 (B1), 8)
      or Shift_Left (Unsigned_32 (B2), 16)
      or Shift_Left (Unsigned_32 (B3), 24));

   procedure Store_BE32 (Val : Unsigned_32; Dst : out Byte_Array)
     with Pre => Dst'Length = 4
   is
   begin
      Dst (Dst'First)     := Byte (Shift_Right (Val, 24) and 16#FF#);
      Dst (Dst'First + 1) := Byte (Shift_Right (Val, 16) and 16#FF#);
      Dst (Dst'First + 2) := Byte (Shift_Right (Val, 8) and 16#FF#);
      Dst (Dst'First + 3) := Byte (Val and 16#FF#);
   end Store_BE32;

   procedure Store_LE32 (Val : Unsigned_32; Dst : out Byte_Array)
     with Pre => Dst'Length = 4
   is
   begin
      Dst (Dst'First)     := Byte (Val and 16#FF#);
      Dst (Dst'First + 1) := Byte (Shift_Right (Val, 8) and 16#FF#);
      Dst (Dst'First + 2) := Byte (Shift_Right (Val, 16) and 16#FF#);
      Dst (Dst'First + 3) := Byte (Shift_Right (Val, 24) and 16#FF#);
   end Store_LE32;

   procedure Store_LE64 (Val : Unsigned_64; Dst : out Byte_Array)
     with Pre => Dst'Length = 8
   is
   begin
      for I in 0 .. 7 loop
         Dst (Dst'First + I) := Byte (Shift_Right (Val, I * 8) and 16#FF#);
      end loop;
   end Store_LE64;

   function Constant_Time_Eq (A, B : Byte_Array) return Boolean is
      Diff : Byte := 0;
   begin
      if A'Length /= B'Length then
         return False;
      end if;
      for I in 0 .. A'Length - 1 loop
         pragma Loop_Invariant
           ((Diff = 0) =
            (for all K in 0 .. I - 1 => A (A'First + K) = B (B'First + K)));
         Diff := Diff or (A (A'First + I) xor B (B'First + I));
      end loop;
      return Diff = 0;
   end Constant_Time_Eq;

   procedure Secure_Zero (Data : out Byte_Array) is
   begin
      for I in Data'Range loop
         pragma Loop_Invariant
           (for all K in Data'First .. I - 1 => Data (K) = 0);
         Data (I) := 0;
      end loop;
   end Secure_Zero;

   procedure Sha256_Compress
     (State : in out Word32_Array;
      Block : Byte_Array)
     with Pre => State'First = 0 and then State'Last = 7
                 and then Block'Length = 64
   is
      W : Word32_Array (0 .. 63) := (others => 0);
      A, B, C, D, E, F, G, H : Unsigned_32;
      S0, S1, Ch, Maj, T1, T2 : Unsigned_32;
   begin
      for I in 0 .. 15 loop
         W (I) := Load_BE32
           (Block (Block'First + I * 4),
            Block (Block'First + I * 4 + 1),
            Block (Block'First + I * 4 + 2),
            Block (Block'First + I * 4 + 3));
      end loop;

      for I in 16 .. 63 loop
         S0 := Rotate_Right (W (I - 15), 7)
               xor Rotate_Right (W (I - 15), 18)
               xor Shift_Right (W (I - 15), 3);
         S1 := Rotate_Right (W (I - 2), 17)
               xor Rotate_Right (W (I - 2), 19)
               xor Shift_Right (W (I - 2), 10);
         W (I) := W (I - 16) + S0 + W (I - 7) + S1;
      end loop;

      A := State (0);
      B := State (1);
      C := State (2);
      D := State (3);
      E := State (4);
      F := State (5);
      G := State (6);
      H := State (7);

      for R in 0 .. 63 loop
         S1  := Rotate_Right (E, 6) xor Rotate_Right (E, 11) xor Rotate_Right (E, 25);
         Ch  := (E and F) xor ((not E) and G);
         T1  := H + S1 + Ch + Sha256_K (R) + W (R);
         S0  := Rotate_Right (A, 2) xor Rotate_Right (A, 13) xor Rotate_Right (A, 22);
         Maj := (A and B) xor (A and C) xor (B and C);
         T2  := S0 + Maj;

         H := G;
         G := F;
         F := E;
         E := D + T1;
         D := C;
         C := B;
         B := A;
         A := T1 + T2;
      end loop;

      State (0) := State (0) + A;
      State (1) := State (1) + B;
      State (2) := State (2) + C;
      State (3) := State (3) + D;
      State (4) := State (4) + E;
      State (5) := State (5) + F;
      State (6) := State (6) + G;
      State (7) := State (7) + H;
   end Sha256_Compress;

   procedure Sha256_Init (Ctx : out Sha256_Context) is
   begin
      Ctx := (State  => Sha256_H0,
              Buffer => (others => 0),
              Filled => 0,
              Total  => 0);
   end Sha256_Init;

   procedure Sha256_Update_Internal
     (Ctx          : in out Sha256_Context;
      Data         : Byte_Array;
      Update_Total : Boolean)
     with Pre  => Ctx.Filled < 64,
          Post => Ctx.Filled < 64
   is
      Offset : Natural := 0;
      Rem_Len : Natural;
      Take   : Natural;
   begin
      if Update_Total then
         Ctx.Total := Ctx.Total + Unsigned_64 (Data'Length);
      end if;

      if Data'Length = 0 then
         return;
      end if;

      if Ctx.Filled > 0 then
         Take := 64 - Ctx.Filled;
         if Take > Data'Length then
            Take := Data'Length;
         end if;
         for I in 0 .. Take - 1 loop
            Ctx.Buffer (Ctx.Filled + I) := Data (Data'First + I);
         end loop;
         Ctx.Filled := Ctx.Filled + Take;
         Offset := Take;
         if Ctx.Filled = 64 then
            Sha256_Compress (Ctx.State, Ctx.Buffer);
            Ctx.Filled := 0;
         end if;
      end if;

      while Offset <= Data'Length and then Data'Length - Offset >= 64 loop
         pragma Loop_Invariant (Ctx.Filled = 0 and then Offset <= Data'Length);
         Sha256_Compress
           (Ctx.State,
            Data (Data'First + Offset .. Data'First + Offset + 63));
         Offset := Offset + 64;
      end loop;

      Rem_Len := Data'Length - Offset;
      if Rem_Len > 0 then
         for I in 0 .. Rem_Len - 1 loop
            Ctx.Buffer (I) := Data (Data'First + Offset + I);
         end loop;
         Ctx.Filled := Rem_Len;
      end if;
   end Sha256_Update_Internal;

   procedure Sha256_Update
     (Ctx  : in out Sha256_Context;
      Data : Byte_Array)
   is
   begin
      Sha256_Update_Internal (Ctx, Data, Update_Total => True);
   end Sha256_Update;

   procedure Sha256_Finalize
     (Ctx    : in out Sha256_Context;
      Digest : out Hash_256)
   is
      Bits    : constant Unsigned_64 := Ctx.Total * 8;
      Pad     : Byte_Array (0 .. 71) := (0 => 16#80#, others => 0);
      Pad_Len : Natural;
      Len_Buf : Byte_Array (0 .. 7) := (others => 0);
   begin
      if Ctx.Filled < 56 then
         Pad_Len := 56 - Ctx.Filled;
      else
         Pad_Len := 120 - Ctx.Filled;
      end if;

      Sha256_Update_Internal (Ctx, Pad (0 .. Pad_Len - 1), Update_Total => False);

      for I in 0 .. 7 loop
         Len_Buf (I) := Byte (Shift_Right (Bits, (7 - I) * 8) and 16#FF#);
      end loop;
      Sha256_Update_Internal (Ctx, Len_Buf, Update_Total => False);

      Digest := (others => 0);
      for I in 0 .. 7 loop
         Store_BE32 (Ctx.State (I), Digest (I * 4 .. I * 4 + 3));
      end loop;
   end Sha256_Finalize;

   procedure Sha256_One_Shot
     (Data   : Byte_Array;
      Digest : out Hash_256)
   is
      Ctx : Sha256_Context;
   begin
      Sha256_Init (Ctx);
      Sha256_Update (Ctx, Data);
      Sha256_Finalize (Ctx, Digest);
   end Sha256_One_Shot;

   procedure Prepare_Hmac_Pads
     (Key  : Byte_Array;
      Ipad : out Byte_Array;
      Opad : out Byte_Array)
     with Pre => Ipad'First = 0 and then Ipad'Last = 63
                 and then Opad'First = 0 and then Opad'Last = 63
   is
      K_Block : Byte_Array (0 .. 63) := (others => 0);
      Hashed  : Hash_256;
   begin
      if Key'Length > 64 then
         Sha256_One_Shot (Key, Hashed);
         K_Block (0 .. 31) := Hashed;
      elsif Key'Length > 0 then
         for I in 0 .. Key'Length - 1 loop
            K_Block (I) := Key (Key'First + I);
         end loop;
      end if;

      for I in 0 .. 63 loop
         Ipad (I) := K_Block (I) xor 16#36#;
         Opad (I) := K_Block (I) xor 16#5C#;
      end loop;
      Secure_Zero (K_Block);
   end Prepare_Hmac_Pads;

   procedure Hmac_Sha256
     (Key     : Byte_Array;
      Message : Byte_Array;
      Mac     : out Hash_256)
   is
      Ipad, Opad : Byte_Array (0 .. 63);
      Inner_Ctx  : Sha256_Context;
      Outer_Ctx  : Sha256_Context;
      Inner_Hash : Hash_256;
   begin
      Prepare_Hmac_Pads (Key, Ipad, Opad);

      Sha256_Init (Inner_Ctx);
      Sha256_Update (Inner_Ctx, Ipad);
      Sha256_Update (Inner_Ctx, Message);
      Sha256_Finalize (Inner_Ctx, Inner_Hash);

      Sha256_Init (Outer_Ctx);
      Sha256_Update (Outer_Ctx, Opad);
      Sha256_Update (Outer_Ctx, Inner_Hash);
      Sha256_Finalize (Outer_Ctx, Mac);

      Secure_Zero (Ipad);
      Secure_Zero (Opad);
   end Hmac_Sha256;

   procedure Pbkdf2_Hmac_Sha256
     (Password   : Byte_Array;
      Salt       : Byte_Array;
      Iterations : Unsigned_32;
      Output     : out Byte_Array)
   is
      Ipad, Opad   : Byte_Array (0 .. 63);
      Base_Inner   : Sha256_Context;
      Base_Outer   : Sha256_Context;
      Inner_Ctx    : Sha256_Context;
      Outer_Ctx    : Sha256_Context;
      U, T         : Hash_256;
      Idx_Bytes    : Byte_Array (0 .. 3);
      Block_Index  : Unsigned_32 := 1;
      Written      : Natural := 0;
      Take         : Natural;
   begin
      Output := (others => 0);
      Prepare_Hmac_Pads (Password, Ipad, Opad);
      Sha256_Init (Base_Inner);
      Sha256_Update (Base_Inner, Ipad);
      Sha256_Init (Base_Outer);
      Sha256_Update (Base_Outer, Opad);

      while Written < Output'Length loop
         pragma Loop_Invariant (Written < Output'Length);
         pragma Loop_Invariant (Base_Inner.Filled < 64 and then Base_Outer.Filled < 64);

         Store_BE32 (Block_Index, Idx_Bytes);

         --  U_1 = PRF(Password, Salt || INT_32_BE(Block_Index))
         Inner_Ctx := Base_Inner;
         Sha256_Update (Inner_Ctx, Salt);
         Sha256_Update (Inner_Ctx, Idx_Bytes);
         Sha256_Finalize (Inner_Ctx, U);

         Outer_Ctx := Base_Outer;
         Sha256_Update (Outer_Ctx, U);
         Sha256_Finalize (Outer_Ctx, U);

         T := U;

         if Iterations > 1 then
            for N in 2 .. Iterations loop
               pragma Loop_Invariant (Base_Inner.Filled < 64 and then Base_Outer.Filled < 64);
               Inner_Ctx := Base_Inner;
               Sha256_Update (Inner_Ctx, U);
               Sha256_Finalize (Inner_Ctx, U);

               Outer_Ctx := Base_Outer;
               Sha256_Update (Outer_Ctx, U);
               Sha256_Finalize (Outer_Ctx, U);

               for I in 0 .. 31 loop
                  T (I) := T (I) xor U (I);
               end loop;
            end loop;
         end if;

         Take := Output'Length - Written;
         if Take > 32 then
            Take := 32;
         end if;
         for I in 0 .. Take - 1 loop
            Output (Output'First + Written + I) := T (I);
         end loop;

         Written := Written + Take;
         Block_Index := Block_Index + 1;
      end loop;

      Secure_Zero (Ipad);
      Secure_Zero (Opad);
      Secure_Zero (U);
      Secure_Zero (T);
   end Pbkdf2_Hmac_Sha256;

   --  =========================================================================
   --  RFC 8439 ChaCha20-Poly1305 AEAD
   --  =========================================================================

   procedure Quarter_Round (A, B, C, D : in out Unsigned_32) is
   begin
      A := A + B; D := Rotate_Left (D xor A, 16);
      C := C + D; B := Rotate_Left (B xor C, 12);
      A := A + B; D := Rotate_Left (D xor A, 8);
      C := C + D; B := Rotate_Left (B xor C, 7);
   end Quarter_Round;

   procedure ChaCha20_Block
     (Key     : Key_256;
      Counter : Unsigned_32;
      Nonce   : Nonce_96;
      Output  : out Byte_Array)
     with Pre => Output'First = 0 and then Output'Last = 63
   is
      Init_State : Word32_Array (0 .. 15);
      Work       : Word32_Array (0 .. 15);
   begin
      Init_State (0) := 16#61707865#;
      Init_State (1) := 16#3320646e#;
      Init_State (2) := 16#79622d32#;
      Init_State (3) := 16#6b206574#;

      for I in 0 .. 7 loop
         Init_State (4 + I) := Load_LE32
           (Key (I * 4), Key (I * 4 + 1), Key (I * 4 + 2), Key (I * 4 + 3));
      end loop;

      Init_State (12) := Counter;
      Init_State (13) := Load_LE32 (Nonce (0), Nonce (1), Nonce (2), Nonce (3));
      Init_State (14) := Load_LE32 (Nonce (4), Nonce (5), Nonce (6), Nonce (7));
      Init_State (15) := Load_LE32 (Nonce (8), Nonce (9), Nonce (10), Nonce (11));

      Work := Init_State;
      for Round in 1 .. 10 loop
         Quarter_Round (Work (0), Work (4), Work (8),  Work (12));
         Quarter_Round (Work (1), Work (5), Work (9),  Work (13));
         Quarter_Round (Work (2), Work (6), Work (10), Work (14));
         Quarter_Round (Work (3), Work (7), Work (11), Work (15));
         Quarter_Round (Work (0), Work (5), Work (10), Work (15));
         Quarter_Round (Work (1), Work (6), Work (11), Work (12));
         Quarter_Round (Work (2), Work (7), Work (8),  Work (13));
         Quarter_Round (Work (3), Work (4), Work (9),  Work (14));
      end loop;

      Output := (others => 0);
      for I in 0 .. 15 loop
         Store_LE32 (Work (I) + Init_State (I), Output (I * 4 .. I * 4 + 3));
      end loop;
   end ChaCha20_Block;

   procedure ChaCha20_Xor
     (Key       : Key_256;
      Counter   : Unsigned_32;
      Nonce     : Nonce_96;
      Input     : Byte_Array;
      Output    : out Byte_Array)
     with Pre => Output'Length = Input'Length
   is
      Block   : Byte_Array (0 .. 63);
      Ctr     : Unsigned_32 := Counter;
      Offset  : Natural := 0;
      Chunk   : Natural;
   begin
      Output := (others => 0);
      while Offset < Input'Length loop
         pragma Loop_Invariant (Offset < Input'Length);
         ChaCha20_Block (Key, Ctr, Nonce, Block);
         Chunk := Input'Length - Offset;
         if Chunk > 64 then
            Chunk := 64;
         end if;
         for I in 0 .. Chunk - 1 loop
            Output (Output'First + Offset + I) :=
              Input (Input'First + Offset + I) xor Block (I);
         end loop;
         Offset := Offset + Chunk;
         Ctr := Ctr + 1;
      end loop;
      Secure_Zero (Block);
   end ChaCha20_Xor;

   --  Poly1305 (RFC 8439) using 5 x 26-bit limbs (radix 2^26, constant-time, no heap).
   type Poly_Limbs is array (0 .. 4) of Unsigned_64;

   type Poly1305_State is record
      R   : Poly_Limbs := (others => 0);
      S   : Word32_Array (0 .. 3) := (others => 0);
      Acc : Poly_Limbs := (others => 0);
   end record;

   procedure Poly1305_Init
     (St  : out Poly1305_State;
      Key : Key_256)
   is
      T0, T1, T2, T3 : Unsigned_32;
   begin
      T0 := Load_LE32 (Key (0),  Key (1),  Key (2),  Key (3))  and 16#0FFF_FFFF#;
      T1 := Load_LE32 (Key (4),  Key (5),  Key (6),  Key (7))  and 16#0FFF_FFFC#;
      T2 := Load_LE32 (Key (8),  Key (9),  Key (10), Key (11)) and 16#0FFF_FFFC#;
      T3 := Load_LE32 (Key (12), Key (13), Key (14), Key (15)) and 16#0FFF_FFFC#;

      St.R (0) := Unsigned_64 (T0) and 16#03FF_FFFF#;
      St.R (1) := (Shift_Right (Unsigned_64 (T0), 26) or Shift_Left (Unsigned_64 (T1), 6)) and 16#03FF_FFFF#;
      St.R (2) := (Shift_Right (Unsigned_64 (T1), 20) or Shift_Left (Unsigned_64 (T2), 12)) and 16#03FF_FFFF#;
      St.R (3) := (Shift_Right (Unsigned_64 (T2), 14) or Shift_Left (Unsigned_64 (T3), 18)) and 16#03FF_FFFF#;
      St.R (4) := Shift_Right (Unsigned_64 (T3), 8) and 16#03FF_FFFF#;

      St.S (0) := Load_LE32 (Key (16), Key (17), Key (18), Key (19));
      St.S (1) := Load_LE32 (Key (20), Key (21), Key (22), Key (23));
      St.S (2) := Load_LE32 (Key (24), Key (25), Key (26), Key (27));
      St.S (3) := Load_LE32 (Key (28), Key (29), Key (30), Key (31));

      St.Acc := (others => 0);
   end Poly1305_Init;

   procedure Poly1305_Block
     (St       : in out Poly1305_State;
      Block    : Byte_Array;
      Is_Final : Boolean)
     with Pre => Block'Length <= 16
   is
      Buf : Byte_Array (0 .. 16) := (others => 0);
      T0, T1, T2, T3 : Unsigned_32;
      H0, H1, H2, H3, H4 : Unsigned_64;
      R0, R1, R2, R3, R4 : Unsigned_64;
      S1, S2, S3, S4     : Unsigned_64;
      D0, D1, D2, D3, D4 : Unsigned_64;
      C                  : Unsigned_64;
   begin
      for I in 0 .. Block'Length - 1 loop
         Buf (I) := Block (Block'First + I);
      end loop;
      Buf (Block'Length) := 1;
      pragma Unreferenced (Is_Final);

      T0 := Load_LE32 (Buf (0),  Buf (1),  Buf (2),  Buf (3));
      T1 := Load_LE32 (Buf (4),  Buf (5),  Buf (6),  Buf (7));
      T2 := Load_LE32 (Buf (8),  Buf (9),  Buf (10), Buf (11));
      T3 := Load_LE32 (Buf (12), Buf (13), Buf (14), Buf (15));

      H0 := St.Acc (0) + (Unsigned_64 (T0) and 16#03FF_FFFF#);
      H1 := St.Acc (1) + ((Shift_Right (Unsigned_64 (T0), 26) or Shift_Left (Unsigned_64 (T1), 6)) and 16#03FF_FFFF#);
      H2 := St.Acc (2) + ((Shift_Right (Unsigned_64 (T1), 20) or Shift_Left (Unsigned_64 (T2), 12)) and 16#03FF_FFFF#);
      H3 := St.Acc (3) + ((Shift_Right (Unsigned_64 (T2), 14) or Shift_Left (Unsigned_64 (T3), 18)) and 16#03FF_FFFF#);
      H4 := St.Acc (4) + Shift_Right (Unsigned_64 (T3), 8) + Shift_Left (Unsigned_64 (Buf (16)), 24);

      R0 := St.R (0);
      R1 := St.R (1);
      R2 := St.R (2);
      R3 := St.R (3);
      R4 := St.R (4);

      S1 := R1 * 5;
      S2 := R2 * 5;
      S3 := R3 * 5;
      S4 := R4 * 5;

      D0 := H0 * R0 + H1 * S4 + H2 * S3 + H3 * S2 + H4 * S1;
      D1 := H0 * R1 + H1 * R0 + H2 * S4 + H3 * S3 + H4 * S2;
      D2 := H0 * R2 + H1 * R1 + H2 * R0 + H3 * S4 + H4 * S3;
      D3 := H0 * R3 + H1 * R2 + H2 * R1 + H3 * R0 + H4 * S4;
      D4 := H0 * R4 + H1 * R3 + H2 * R2 + H3 * R1 + H4 * R0;

      C := Shift_Right (D0, 26); H0 := D0 and 16#03FF_FFFF#;
      D1 := D1 + C; C := Shift_Right (D1, 26); H1 := D1 and 16#03FF_FFFF#;
      D2 := D2 + C; C := Shift_Right (D2, 26); H2 := D2 and 16#03FF_FFFF#;
      D3 := D3 + C; C := Shift_Right (D3, 26); H3 := D3 and 16#03FF_FFFF#;
      D4 := D4 + C; C := Shift_Right (D4, 26); H4 := D4 and 16#03FF_FFFF#;
      H0 := H0 + C * 5; C := Shift_Right (H0, 26); H0 := H0 and 16#03FF_FFFF#;
      H1 := H1 + C;

      St.Acc := (H0, H1, H2, H3, H4);
   end Poly1305_Block;

   procedure Poly1305_Update_Padded
     (St   : in out Poly1305_State;
      Data : Byte_Array)
   is
      Offset : Natural := 0;
      Rem_L  : Natural;
      Pad16  : Byte_Array (0 .. 15) := (others => 0);
   begin
      while Offset <= Data'Length and then Data'Length - Offset >= 16 loop
         pragma Loop_Invariant (Offset <= Data'Length);
         Poly1305_Block
           (St, Data (Data'First + Offset .. Data'First + Offset + 15), False);
         Offset := Offset + 16;
      end loop;

      Rem_L := Data'Length - Offset;
      if Rem_L > 0 then
         for I in 0 .. Rem_L - 1 loop
            Pad16 (I) := Data (Data'First + Offset + I);
         end loop;
         --  RFC 8439: pad AAD and Ciphertext with 0 bytes to 16-byte boundary.
         Poly1305_Block (St, Pad16, False);
      end if;
   end Poly1305_Update_Padded;

   procedure Poly1305_Finish
     (St  : in out Poly1305_State;
      Tag : out Tag_128)
   is
      H0, H1, H2, H3, H4 : Unsigned_64;
      G0, G1, G2, G3, G4 : Unsigned_64;
      C, Mask            : Unsigned_64;
      F0, F1, F2, F3     : Unsigned_64;
   begin
      H0 := St.Acc (0);
      H1 := St.Acc (1);
      H2 := St.Acc (2);
      H3 := St.Acc (3);
      H4 := St.Acc (4);

      C := Shift_Right (H1, 26); H1 := H1 and 16#03FF_FFFF#;
      H2 := H2 + C; C := Shift_Right (H2, 26); H2 := H2 and 16#03FF_FFFF#;
      H3 := H3 + C; C := Shift_Right (H3, 26); H3 := H3 and 16#03FF_FFFF#;
      H4 := H4 + C; C := Shift_Right (H4, 26); H4 := H4 and 16#03FF_FFFF#;
      H0 := H0 + C * 5; C := Shift_Right (H0, 26); H0 := H0 and 16#03FF_FFFF#;
      H1 := H1 + C;

      --  Compute h + 5 - 2^130
      G0 := H0 + 5; C := Shift_Right (G0, 26); G0 := G0 and 16#03FF_FFFF#;
      G1 := H1 + C; C := Shift_Right (G1, 26); G1 := G1 and 16#03FF_FFFF#;
      G2 := H2 + C; C := Shift_Right (G2, 26); G2 := G2 and 16#03FF_FFFF#;
      G3 := H3 + C; C := Shift_Right (G3, 26); G3 := G3 and 16#03FF_FFFF#;
      G4 := H4 + C - Shift_Left (1, 26);

      --  Select h if g4 bit 63 is set (negative), else g
      Mask := Shift_Right (G4, 63) - 1;
      G0 := G0 and Mask; G1 := G1 and Mask; G2 := G2 and Mask; G3 := G3 and Mask; G4 := G4 and Mask;
      Mask := not Mask;
      H0 := (H0 and Mask) or G0;
      H1 := (H1 and Mask) or G1;
      H2 := (H2 and Mask) or G2;
      H3 := (H3 and Mask) or G3;
      H4 := (H4 and Mask) or G4;

      --  Reassemble 4 x 32-bit words and add s
      F0 := (H0 or Shift_Left (H1, 26)) and 16#FFFF_FFFF#;
      F1 := (Shift_Right (H1, 6) or Shift_Left (H2, 20)) and 16#FFFF_FFFF#;
      F2 := (Shift_Right (H2, 12) or Shift_Left (H3, 14)) and 16#FFFF_FFFF#;
      F3 := (Shift_Right (H3, 18) or Shift_Left (H4, 8)) and 16#FFFF_FFFF#;

      F0 := F0 + Unsigned_64 (St.S (0));
      F1 := F1 + Unsigned_64 (St.S (1)) + Shift_Right (F0, 32);
      F2 := F2 + Unsigned_64 (St.S (2)) + Shift_Right (F1, 32);
      F3 := F3 + Unsigned_64 (St.S (3)) + Shift_Right (F2, 32);

      Tag := (others => 0);
      Store_LE32 (Unsigned_32 (F0 and 16#FFFF_FFFF#), Tag (0 .. 3));
      Store_LE32 (Unsigned_32 (F1 and 16#FFFF_FFFF#), Tag (4 .. 7));
      Store_LE32 (Unsigned_32 (F2 and 16#FFFF_FFFF#), Tag (8 .. 11));
      Store_LE32 (Unsigned_32 (F3 and 16#FFFF_FFFF#), Tag (12 .. 15));
   end Poly1305_Finish;

   procedure Compute_AEAD_Tag
     (Key        : Key_256;
      Nonce      : Nonce_96;
      AAD        : Byte_Array;
      Ciphertext : Byte_Array;
      Tag        : out Tag_128)
   is
      Block0    : Byte_Array (0 .. 63);
      Otk       : Key_256;
      St        : Poly1305_State;
      Len_Block : Byte_Array (0 .. 15);
   begin
      ChaCha20_Block (Key, 0, Nonce, Block0);
      Otk := Block0 (0 .. 31);
      Poly1305_Init (St, Otk);
      Poly1305_Update_Padded (St, AAD);
      Poly1305_Update_Padded (St, Ciphertext);

      Store_LE64 (Unsigned_64 (AAD'Length), Len_Block (0 .. 7));
      Store_LE64 (Unsigned_64 (Ciphertext'Length), Len_Block (8 .. 15));
      Poly1305_Block (St, Len_Block, True);
      Poly1305_Finish (St, Tag);

      Secure_Zero (Block0);
      Secure_Zero (Otk);
   end Compute_AEAD_Tag;

   procedure ChaCha20_Poly1305_Encrypt
     (Key        : Key_256;
      Nonce      : Nonce_96;
      AAD        : Byte_Array;
      Plaintext  : Byte_Array;
      Ciphertext : out Byte_Array;
      Tag        : out Tag_128)
   is
   begin
      ChaCha20_Xor (Key, 1, Nonce, Plaintext, Ciphertext);
      Compute_AEAD_Tag (Key, Nonce, AAD, Ciphertext, Tag);
   end ChaCha20_Poly1305_Encrypt;

   procedure ChaCha20_Poly1305_Decrypt
     (Key        : Key_256;
      Nonce      : Nonce_96;
      AAD        : Byte_Array;
      Ciphertext : Byte_Array;
      Tag        : Tag_128;
      Plaintext  : out Byte_Array;
      Valid      : out Boolean)
   is
      Expected_Tag : Tag_128;
   begin
      Compute_AEAD_Tag (Key, Nonce, AAD, Ciphertext, Expected_Tag);
      if Constant_Time_Eq (Expected_Tag, Tag) then
         ChaCha20_Xor (Key, 1, Nonce, Ciphertext, Plaintext);
         Valid := True;
      else
         Secure_Zero (Plaintext);
         Valid := False;
      end if;
      Secure_Zero (Expected_Tag);
   end ChaCha20_Poly1305_Decrypt;

end NC_Security_Crypto;
