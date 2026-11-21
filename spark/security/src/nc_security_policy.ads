--  NetizenCore SPARK Ada Security Core — Security Policy Rules
--  Owns trust-tier resolution, built-in identity protection, and
--  IPC / process-operation authorization policy.

pragma SPARK_Mode (On);

with Interfaces;               use Interfaces;
with NC_Security_Types;        use NC_Security_Types;
with NC_Security_Capabilities; use NC_Security_Capabilities;

package NC_Security_Policy
  with Pure
is
   --  Protected built-in system identities that third-party packages
   --  must NEVER impersonate.
   function Is_Reserved_System_Id (Id : Bounded_App_Id) return Boolean;

   --  Determine trust tier from installation source and verified platform signature.
   --  System tier is ONLY granted when BOTH:
   --    1) Package comes from read-only SystemImage, AND
   --    2) Package signature is verified against a trusted platform key.
   function Resolve_Trust_Tier
     (Source          : Install_Source;
      Is_Verified     : Boolean;
      Platform_Signed : Boolean) return Trust_Tier
     with Post =>
       (if Source /= Source_System_Image
          or else not Is_Verified
          or else not Platform_Signed
        then Resolve_Trust_Tier'Result = Tier_User);

   --  Compute granted capabilities for a package based on tier and policy.
   function Package_Granted_Caps
     (Tier      : Trust_Tier;
      Declared  : Cap_Mask;
      Approved  : Cap_Mask) return Cap_Mask
     with Post =>
       (if Tier = Tier_User then
          (Package_Granted_Caps'Result and System_Only_Mask) = 0)
       and then Is_Valid_Mask (Package_Granted_Caps'Result);

   --  Authorize a process launch operation.
   function Authorize_Process_Launch
     (Caller_Id   : Bounded_App_Id;
      Caller_Caps : Cap_Mask;
      Target_Id   : Bounded_App_Id) return Auth_Decision
     with Post =>
       (if not Is_Valid_App_Id (Caller_Id) or else not Is_Valid_App_Id (Target_Id) then
          Authorize_Process_Launch'Result /= Auth_Allow)
       and then
       (if not Has_Capability (Caller_Caps, Cap_Process_Launch) then
          Authorize_Process_Launch'Result /= Auth_Allow);

   --  Authorize an inter-application IPC operation.
   function Authorize_IPC
     (Sender_Id   : Bounded_App_Id;
      Sender_Caps : Cap_Mask;
      Target_Id   : Bounded_App_Id;
      Privileged  : Boolean) return Auth_Decision
     with Post =>
       (if not Is_Valid_App_Id (Sender_Id) or else not Is_Valid_App_Id (Target_Id) then
          Authorize_IPC'Result /= Auth_Allow)
       and then
       (if not Has_Capability (Sender_Caps, Cap_Ipc_Endpoint) then
          Authorize_IPC'Result /= Auth_Allow)
       and then
       (if Privileged and then not Has_Capability (Sender_Caps, Cap_Security_Manage) then
          Authorize_IPC'Result /= Auth_Allow);

end NC_Security_Policy;
