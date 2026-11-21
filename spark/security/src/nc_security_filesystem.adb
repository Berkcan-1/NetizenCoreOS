--  NetizenCore SPARK Ada Security Core — Authoritative Filesystem Access Policy Body

pragma SPARK_Mode (On);

with Interfaces; use Interfaces;

package body NC_Security_Filesystem is

   type Segment_Span is record
      First : Positive := 1;
      Last  : Natural  := 0;
   end record;

   subtype Segment_Index is Natural range 1 .. Max_Path_Depth;
   type Segment_Array is array (Segment_Index) of Segment_Span;

   function Segment_Equals
     (Path : Bounded_Path;
      Seg  : Segment_Span;
      Lit  : String) return Boolean
     with Pre =>
       Seg.First <= Max_Path_Len
       and then Seg.Last <= Path.Len
       and then Seg.First <= Seg.Last
   is
      Seg_Len : constant Natural := Seg.Last - Seg.First + 1;
   begin
      if Seg_Len /= Lit'Length then
         return False;
      end if;
      for I in 0 .. Seg_Len - 1 loop
         if Path.Data (Seg.First + I) /= Character'Pos (Lit (Lit'First + I)) then
            return False;
         end if;
      end loop;
      return True;
   end Segment_Equals;

   function Segment_Equals_App_Id
     (Path : Bounded_Path;
      Seg  : Segment_Span;
      Id   : Bounded_App_Id) return Boolean
     with Pre =>
       Seg.First <= Max_Path_Len
       and then Seg.Last <= Path.Len
       and then Seg.First <= Seg.Last
   is
      Seg_Len : constant Natural := Seg.Last - Seg.First + 1;
   begin
      if Id.Len = 0 or else Seg_Len /= Id.Len then
         return False;
      end if;
      for I in 0 .. Seg_Len - 1 loop
         if Path.Data (Seg.First + I) /= Id.Data (I + 1) then
            return False;
         end if;
      end loop;
      return True;
   end Segment_Equals_App_Id;

   procedure Split_Path
     (Path  : Bounded_Path;
      Segs  : out Segment_Array;
      Count : out Natural;
      Valid : out Boolean)
     with Post =>
       Count <= Max_Path_Depth
       and then
       (if Path.Len = 1 and then Path.Data (1) = Character'Pos ('/') then
          Valid and then Count = 0)
       and then
       (if Valid then
          Path.Len >= 1
          and then Path.Data (1) = Character'Pos ('/')
          and then (for all I in 1 .. Path.Len =>
                      Path.Data (I) /= 0
                      and then Path.Data (I) /= Character'Pos ('\'))
          and then (for all S in 1 .. Count =>
                      Segs (S).First <= Max_Path_Len
                      and then Segs (S).Last <= Path.Len
                      and then Segs (S).First <= Segs (S).Last))
   is
      Effective_Len : Natural;
      Pos           : Positive;
      Seg_Start     : Positive;
      Seg_End       : Natural;
   begin
      Segs  := (others => (First => 1, Last => 0));
      Count := 0;
      Valid := False;

      if Path.Len = 0 or else Path.Data (1) /= Character'Pos ('/') then
         return;
      end if;

      for I in 1 .. Path.Len loop
         pragma Loop_Invariant
           (for all K in 1 .. I - 1 =>
              Path.Data (K) /= 0 and then Path.Data (K) /= Character'Pos ('\'));
         if Path.Data (I) = 0 or else Path.Data (I) = Character'Pos ('\') then
            return;
         end if;
      end loop;

      if Path.Len = 1 then
         Valid := True;
         return;
      end if;

      --  Strip at most one trailing '/' (matching nc-fs-policy behavior).
      Effective_Len := Path.Len;
      if Path.Data (Effective_Len) = Character'Pos ('/') then
         Effective_Len := Effective_Len - 1;
      end if;

      if Effective_Len = 1 then
         --  Path was "//" -> invalid empty segment!
         return;
      end if;

      Pos := 2;
      while Pos <= Effective_Len loop
         pragma Loop_Variant (Increases => Pos);
         pragma Loop_Invariant (Effective_Len <= Path.Len);
         pragma Loop_Invariant (Pos >= 2 and then Pos <= Effective_Len);
         pragma Loop_Invariant (Count <= Max_Path_Depth);
         pragma Loop_Invariant
           (for all S in 1 .. Count =>
              Segs (S).First <= Max_Path_Len
              and then Segs (S).Last <= Path.Len
              and then Segs (S).First <= Segs (S).Last);

         Seg_Start := Pos;
         while Pos <= Effective_Len
           and then Path.Data (Pos) /= Character'Pos ('/')
         loop
            pragma Loop_Variant (Increases => Pos);
            pragma Loop_Invariant (Effective_Len <= Path.Len);
            pragma Loop_Invariant (Seg_Start >= 2);
            pragma Loop_Invariant (Pos >= Seg_Start and then Pos <= Effective_Len);
            pragma Loop_Invariant (Count <= Max_Path_Depth);
            pragma Loop_Invariant
              (for all S in 1 .. Count =>
                 Segs (S).First <= Max_Path_Len
                 and then Segs (S).Last <= Path.Len
                 and then Segs (S).First <= Segs (S).Last);
            Pos := Pos + 1;
         end loop;

         Seg_End := Pos - 1;

         --  Reject empty segment (e.g. "//")
         if Seg_End < Seg_Start then
            return;
         end if;

         --  Reject "." and ".."
         if Seg_End = Seg_Start
           and then Path.Data (Seg_Start) = Character'Pos ('.')
         then
            return;
         end if;

         if Seg_End = Seg_Start + 1
           and then Path.Data (Seg_Start) = Character'Pos ('.')
           and then Path.Data (Seg_Start + 1) = Character'Pos ('.')
         then
            return;
         end if;

         if Count >= Max_Path_Depth then
            return;
         end if;

         Count := Count + 1;
         Segs (Count) := (First => Seg_Start, Last => Seg_End);

         if Pos <= Effective_Len then
            --  Pos is currently at '/', advance past it; if trailing '/', it's empty
            if Pos = Effective_Len then
               return;
            end if;
            Pos := Pos + 1;
         end if;
      end loop;

      Valid := True;
   end Split_Path;

   function Is_Safe_Path (Path : Bounded_Path) return Boolean is
      Segs  : Segment_Array;
      Count : Natural;
      Valid : Boolean;
   begin
      Split_Path (Path, Segs, Count, Valid);
      return Valid;
   end Is_Safe_Path;

   function Need (Caps : Cap_Mask; Bit : Cap_Mask) return Fs_Decision is
   begin
      if (Caps and Bit) /= 0 then
         return Fs_Allow;
      else
         return Fs_Deny_Permission;
      end if;
   end Need;

   function Need_Any
     (Caps   : Cap_Mask;
      First  : Cap_Mask;
      Second : Cap_Mask) return Fs_Decision
   is
   begin
      if (Caps and (First or Second)) /= 0 then
         return Fs_Allow;
      else
         return Fs_Deny_Permission;
      end if;
   end Need_Any;

   function All_Access (Caps : Cap_Mask; Op : Fs_Op) return Fs_Decision is
   begin
      case Op is
         when Fs_Op_Read =>
            return Need (Caps, Cap_Fs_All_Read);
         when Fs_Op_Write | Fs_Op_Delete =>
            return Need (Caps, Cap_Fs_All_Write);
      end case;
   end All_Access;

   function Check_Access
     (Caller_Id   : Bounded_App_Id;
      Caller_Caps : Cap_Mask;
      Path        : Bounded_Path;
      Op          : Fs_Op) return Fs_Decision
   is
      Segs  : Segment_Array;
      Count : Natural;
      Valid : Boolean;
   begin
      if not Is_Safe_Path (Path) then
         return Fs_Deny_Bad_Path;
      end if;
      Split_Path (Path, Segs, Count, Valid);
      if not Valid then
         return Fs_Deny_Bad_Path;
      end if;

      --  Root "/"
      if Count = 0 then
         if Op = Fs_Op_Read then
            return Fs_Allow;
         else
            return Fs_Deny_Read_Only;
         end if;
      end if;

      --  "/system" and "/system/..." — strictly read-only, requires FS_ALL_READ
      if Segment_Equals (Path, Segs (1), "system") then
         if Op /= Fs_Op_Read then
            return Fs_Deny_Read_Only;
         else
            return Need (Caller_Caps, Cap_Fs_All_Read);
         end if;
      end if;

      --  "/data" and subtrees
      if Segment_Equals (Path, Segs (1), "data") then
         if Count = 1 then
            if Op = Fs_Op_Read then
               return Fs_Allow;
            else
               return Fs_Deny_Permission;
            end if;
         end if;

         --  "/data/app/..."
         if Segment_Equals (Path, Segs (2), "app") then
            return All_Access (Caller_Caps, Op);
         end if;

         --  "/data/appdata" and "/data/appdata/<owner>/..."
         if Segment_Equals (Path, Segs (2), "appdata") then
            if Count = 2 then
               if Op = Fs_Op_Read then
                  return Need (Caller_Caps, Cap_Fs_All_Read);
               else
                  return Fs_Deny_Permission;
               end if;
            else
               --  Count >= 3: Segs (3) is <owner>
               --  Protected kernel-only security and system-service databases:
               --  NO Ring 3 process (even with FS_ALL_READ / FS_ALL_WRITE) may
               --  bypass the permission manager or tamper with them via VFS.
               if Segment_Equals (Path, Segs (3), "nc.security")
                 or else Segment_Equals (Path, Segs (3), "nc.contacts.db")
               then
                  return Fs_Deny_Permission;
               end if;
               if Segment_Equals_App_Id (Path, Segs (3), Caller_Id) then
                  return Fs_Allow;
               else
                  return All_Access (Caller_Caps, Op);
               end if;
            end if;
         end if;

         --  "/data/media/..."
         if Segment_Equals (Path, Segs (2), "media") then
            case Op is
               when Fs_Op_Read =>
                  return Need_Any (Caller_Caps, Cap_Fs_User_Read, Cap_Fs_All_Read);
               when Fs_Op_Write =>
                  return Need_Any (Caller_Caps, Cap_Fs_User_Write, Cap_Fs_All_Write);
               when Fs_Op_Delete =>
                  return Need_Any (Caller_Caps, Cap_Fs_User_Delete, Cap_Fs_All_Write);
            end case;
         end if;

         return Fs_Deny_Permission;
      end if;

      --  "/storage" and "/storage/usb0/..."
      if Segment_Equals (Path, Segs (1), "storage") then
         if Count = 1 then
            if Op = Fs_Op_Read then
               return Need_Any (Caller_Caps, Cap_Fs_User_Read, Cap_Fs_All_Read);
            else
               return Fs_Deny_Read_Only;
            end if;
         end if;

         if Segment_Equals (Path, Segs (2), "usb0") then
            case Op is
               when Fs_Op_Read =>
                  return Need_Any (Caller_Caps, Cap_Fs_User_Read, Cap_Fs_All_Read);
               when Fs_Op_Write =>
                  return Need_Any (Caller_Caps, Cap_Fs_User_Write, Cap_Fs_All_Write);
               when Fs_Op_Delete =>
                  return Need_Any (Caller_Caps, Cap_Fs_User_Delete, Cap_Fs_All_Write);
            end case;
         end if;

         return Fs_Deny_Permission;
      end if;

      return Fs_Deny_Permission;
   end Check_Access;

end NC_Security_Filesystem;
