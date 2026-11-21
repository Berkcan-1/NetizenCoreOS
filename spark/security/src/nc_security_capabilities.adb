--  NetizenCore SPARK Ada Security Core — Authoritative Capabilities Body

pragma SPARK_Mode (On);

package body NC_Security_Capabilities is

   function Clamp_User (Declared : Cap_Mask) return Cap_Mask is
   begin
      return Declared and User_Allowed_Mask;
   end Clamp_User;

   function Dropped_For_User (Declared : Cap_Mask) return Cap_Mask is
   begin
      return Declared and (not User_Allowed_Mask);
   end Dropped_For_User;

   function Compute_Effective_Grant
     (Requested : Cap_Mask;
      Approved  : Cap_Mask;
      Ceiling   : Cap_Mask) return Cap_Mask
   is
   begin
      return Requested and Approved and Ceiling and All_Known_Caps;
   end Compute_Effective_Grant;

   function Has_Capability (Mask : Cap_Mask; Cap : Cap_Mask) return Boolean is
   begin
      if not Is_Single_Known_Cap (Cap) then
         return False;
      end if;
      return (Mask and Cap) /= 0;
   end Has_Capability;

end NC_Security_Capabilities;
