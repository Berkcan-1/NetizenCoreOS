--  NetizenCore Zero-Footprint Bare-Metal Interfaces.C (LP64: AArch64 & x86_64)
--  Contains only fixed-width C ABI scalar types; zero exceptions, zero heap.

package Interfaces.C is
   pragma Pure;

   type int            is new Integer;
   type short          is new Short_Integer;
   type long           is new Long_Integer;
   type signed_char    is range -128 .. 127;
   for signed_char'Size use 8;

   type unsigned       is mod 2 ** int'Size;
   type unsigned_short is mod 2 ** short'Size;
   type unsigned_long  is mod 2 ** long'Size;
   type unsigned_char  is mod 2 ** 8;
   for unsigned_char'Size use 8;

   type size_t         is mod 2 ** 64;
   for size_t'Size use 64;

   type ptrdiff_t      is range -(2 ** 63) .. (2 ** 63) - 1;
   for ptrdiff_t'Size use 64;

   type char           is new Character;
   nul : constant char := char'First;

end Interfaces.C;
