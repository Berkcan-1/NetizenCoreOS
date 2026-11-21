--  NetizenCore SPARK Ada Security Core — Authoritative Filesystem Access Policy
--
--  Replaces `aarch64crates/runtime/nc-fs-policy` as the single authoritative
--  filesystem authorization engine.
--
--  Formally verified invariants:
--    - Relative paths, empty paths, NUL bytes, and backslashes are rejected.
--    - "." and ".." traversal and empty path components ("//") are rejected.
--    - "/" and "/system/..." are NEVER writable or deletable (even for System tier).
--    - "/data/appdata/<owner>/..." grants automatic ownership access ONLY when
--      Caller_Id is non-empty and EXACTLY equals <owner>.
--    - User-tier capabilities cannot bypass system-only paths ("/data/app", "/system").

pragma SPARK_Mode (On);

with Interfaces;               use Interfaces;
with NC_Security_Types;        use NC_Security_Types;
with NC_Security_Capabilities; use NC_Security_Capabilities;

package NC_Security_Filesystem
  with Pure
is
   subtype Path_Length is Natural range 0 .. Max_Path_Len;
   subtype Path_Buf    is Byte_Array (1 .. Max_Path_Len);

   type Bounded_Path is record
      Len  : Path_Length := 0;
      Data : Path_Buf    := (others => 0);
   end record;

   --  Check whether a path is syntactically safe (starts with '/', no NUL, no '\',
   --  no empty components, no '.' or '..' traversal, depth <= Max_Path_Depth).
   function Is_Safe_Path (Path : Bounded_Path) return Boolean
     with Post =>
       (if Is_Safe_Path'Result then
          Path.Len >= 1
          and then Path.Data (1) = Character'Pos ('/')
          and then (for all I in 1 .. Path.Len =>
                      Path.Data (I) /= 0
                      and then Path.Data (I) /= Character'Pos ('\')))
       and then
       (if Path.Len = 1 and then Path.Data (1) = Character'Pos ('/') then
          Is_Safe_Path'Result);

   --  Authoritative filesystem authorization decision.
   function Check_Access
     (Caller_Id   : Bounded_App_Id;
      Caller_Caps : Cap_Mask;
      Path        : Bounded_Path;
      Op          : Fs_Op) return Fs_Decision
     with Post =>
       (if not Is_Safe_Path (Path) then
          Check_Access'Result = Fs_Deny_Bad_Path)
       and then
       (if Path.Len = 1 and then Path.Data (1) = Character'Pos ('/')
           and then Op /= Fs_Op_Read
        then Check_Access'Result = Fs_Deny_Read_Only);

end NC_Security_Filesystem;
