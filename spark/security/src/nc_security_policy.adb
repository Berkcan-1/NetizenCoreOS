--  NetizenCore SPARK Ada Security Core — Security Policy Rules Body

pragma SPARK_Mode (On);

package body NC_Security_Policy is

   function Is_Reserved_System_Id (Id : Bounded_App_Id) return Boolean is
   begin
      return Equal_App_Id (Id, Make_App_Id ("nc.kernel"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.home"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.settings"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.files"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.package-installer"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.keyboard"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.gallery"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.messages"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.phone"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.contacts"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.camera"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.system"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.stopwatch"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.browser"))
        or else Equal_App_Id (Id, Make_App_Id ("nc.security"));
   end Is_Reserved_System_Id;

   function Resolve_Trust_Tier
     (Source          : Install_Source;
      Is_Verified     : Boolean;
      Platform_Signed : Boolean) return Trust_Tier
   is
   begin
      if Source = Source_System_Image
        and then Is_Verified
        and then Platform_Signed
      then
         return Tier_System;
      else
         return Tier_User;
      end if;
   end Resolve_Trust_Tier;

   function Package_Granted_Caps
     (Tier      : Trust_Tier;
      Declared  : Cap_Mask;
      Approved  : Cap_Mask) return Cap_Mask
   is
   begin
      case Tier is
         when Tier_System =>
            return Compute_Effective_Grant (Declared, Approved, All_Known_Caps);
         when Tier_User =>
            return Compute_Effective_Grant (Declared, Approved, User_Allowed_Mask);
      end case;
   end Package_Granted_Caps;

   function Authorize_Process_Launch
     (Caller_Id   : Bounded_App_Id;
      Caller_Caps : Cap_Mask;
      Target_Id   : Bounded_App_Id) return Auth_Decision
   is
   begin
      if not Is_Valid_App_Id (Caller_Id) or else not Is_Valid_App_Id (Target_Id) then
         return Auth_Deny_Invalid_Input;
      end if;
      if not Has_Capability (Caller_Caps, Cap_Process_Launch) then
         return Auth_Deny_Capability_Not_Granted;
      end if;
      return Auth_Allow;
   end Authorize_Process_Launch;

   function Authorize_IPC
     (Sender_Id   : Bounded_App_Id;
      Sender_Caps : Cap_Mask;
      Target_Id   : Bounded_App_Id;
      Privileged  : Boolean) return Auth_Decision
   is
   begin
      if not Is_Valid_App_Id (Sender_Id) or else not Is_Valid_App_Id (Target_Id) then
         return Auth_Deny_Invalid_Input;
      end if;
      if not Has_Capability (Sender_Caps, Cap_Ipc_Endpoint) then
         return Auth_Deny_Capability_Not_Granted;
      end if;
      if Privileged and then not Has_Capability (Sender_Caps, Cap_Security_Manage) then
         return Auth_Deny_System_Only;
      end if;
      return Auth_Allow;
   end Authorize_IPC;

end NC_Security_Policy;
