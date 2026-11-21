--  NetizenCore SPARK Ada Security Core — Authoritative Security Types Body

pragma SPARK_Mode (On);

package body NC_Security_Types is

   function Equal_App_Id (A, B : Bounded_App_Id) return Boolean is
   begin
      if A.Len /= B.Len then
         return False;
      end if;
      for I in 1 .. A.Len loop
         pragma Loop_Invariant
           (for all K in 1 .. I - 1 => A.Data (K) = B.Data (K));
         if A.Data (I) /= B.Data (I) then
            return False;
         end if;
      end loop;
      return True;
   end Equal_App_Id;

   function Is_Safe_Id_Char (C : Byte) return Boolean is
     ((C >= Character'Pos ('a') and then C <= Character'Pos ('z'))
      or else (C >= Character'Pos ('A') and then C <= Character'Pos ('Z'))
      or else (C >= Character'Pos ('0') and then C <= Character'Pos ('9'))
      or else C = Character'Pos ('.')
      or else C = Character'Pos ('-')
      or else C = Character'Pos ('_'));

   function Is_Valid_App_Id (Id : Bounded_App_Id) return Boolean is
   begin
      if Id.Len = 0 then
         return False;
      end if;
      --  Disallow leading or trailing dot.
      if Id.Data (1) = Character'Pos ('.')
        or else Id.Data (Id.Len) = Character'Pos ('.')
      then
         return False;
      end if;
      for I in 1 .. Id.Len loop
         pragma Loop_Invariant
           (for all K in 1 .. I - 1 =>
              Is_Safe_Id_Char (Id.Data (K)) and then
              Id.Data (K) /= 0 and then
              Id.Data (K) /= Character'Pos ('/') and then
              Id.Data (K) /= Character'Pos ('\'));
         if not Is_Safe_Id_Char (Id.Data (I)) then
            return False;
         end if;
         --  Disallow consecutive dots ("..")
         if I > 1
           and then Id.Data (I) = Character'Pos ('.')
           and then Id.Data (I - 1) = Character'Pos ('.')
         then
            return False;
         end if;
      end loop;
      return True;
   end Is_Valid_App_Id;

   function Make_App_Id (S : String) return Bounded_App_Id is
      Res : Bounded_App_Id := Empty_App_Id;
   begin
      Res.Len := S'Length;
      for I in 1 .. S'Length loop
         pragma Loop_Invariant (Res.Len = S'Length);
         Res.Data (I) := Character'Pos (S (S'First + (I - 1)));
      end loop;
      return Res;
   end Make_App_Id;

   function Make_Publisher (S : String) return Bounded_Publisher is
      Res : Bounded_Publisher := Empty_Publisher;
   begin
      Res.Len := S'Length;
      for I in 1 .. S'Length loop
         pragma Loop_Invariant (Res.Len = S'Length);
         Res.Data (I) := Character'Pos (S (S'First + (I - 1)));
      end loop;
      return Res;
   end Make_Publisher;

end NC_Security_Types;
