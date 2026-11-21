--  NetizenCore SPARK Ada Security Core — DysonSphere Central Authorization Engine
--
--  Authoritative security engine for NetizenCore.
--  Design invariants:
--    1. No global active_app_id string (eliminates identity confusion & races).
--       Every authorization request MUST carry an explicit trusted caller identity
--       supplied by the kernel's current task record.
--    2. Default-DENY: uninitialized engine, unknown app, unknown capability,
--       revoked app, or rollback version ALWAYS returns a DENY decision.
--    3. Requested capabilities, user-approved capabilities, and granted
--       capabilities are strictly distinct fields.
--    4. Bounded static storage (32 apps, 64-entry audit ring); zero heap allocation.

pragma SPARK_Mode (On);

with Interfaces;               use Interfaces;
with NC_Security_Types;        use NC_Security_Types;
with NC_Security_Capabilities; use NC_Security_Capabilities;

package NC_Security_DysonSphere
  with Preelaborate
is
   type App_Identity_Record is record
      In_Use              : Boolean           := False;
      App_Id              : Bounded_App_Id    := Empty_App_Id;
      Publisher           : Bounded_Publisher := Empty_Publisher;
      Version_Code        : Unsigned_32       := 0;
      Min_Allowed_Version : Unsigned_32       := 0;
      Payload_Hash        : Hash_256          := Zero_Hash_256;
      Requested_Caps      : Cap_Mask          := Cap_None;
      Approved_Caps       : Cap_Mask          := Cap_None;
      Granted_Caps        : Cap_Mask          := Cap_None;
      Tier                : Trust_Tier        := Tier_User;
      Revoked             : Boolean           := False;
      Is_Builtin          : Boolean           := False;
   end record;

   type Audit_Event is record
      Seq           : Unsigned_32   := 0;
      App_Id_Tag    : Audit_Tag_Buf := Zero_Audit_Tag;
      Requested_Cap : Cap_Mask      := Cap_None;
      Decision      : Auth_Decision := Auth_Deny_Policy_Locked;
   end record;

   subtype App_Slot_Index   is Natural range 1 .. Max_Registered_Apps;
   subtype Audit_Ring_Index is Natural range 0 .. Audit_Ring_Capacity - 1;

   type App_Table  is array (App_Slot_Index) of App_Identity_Record;
   type Audit_Ring is array (Audit_Ring_Index) of Audit_Event;

   type Engine_State is record
      Initialized     : Boolean          := False;
      Policy_Version  : Unsigned_32      := 1;
      Apps            : App_Table        := (others => <>);
      App_Count       : Natural          := 0;
      Audit           : Audit_Ring       := (others => <>);
      Audit_Write_Idx : Audit_Ring_Index := 0;
      Audit_Seq       : Unsigned_32      := 0;
      Denied_Count    : Unsigned_64      := 0;
   end record;

   --  Explicit authorization request (always includes trusted caller identity).
   type Auth_Request is record
      Caller_Id     : Bounded_App_Id := Empty_App_Id;
      Requested_Cap : Cap_Mask       := Cap_None;
   end record;

   --  Ghost/specification query: does an app exist in the table?
   function Find_App_Slot
     (State  : Engine_State;
      App_Id : Bounded_App_Id) return Natural
     with Post =>
       Find_App_Slot'Result <= Max_Registered_Apps
       and then
       (if Find_App_Slot'Result in App_Slot_Index then
          State.Apps (Find_App_Slot'Result).In_Use
          and then Equal_App_Id (State.Apps (Find_App_Slot'Result).App_Id, App_Id)
        else
          (for all I in App_Slot_Index =>
             not (State.Apps (I).In_Use
                  and then Equal_App_Id (State.Apps (I).App_Id, App_Id))));

   --  Initialize DysonSphere and seed least-privilege built-in policies.
   procedure Initialize (State : out Engine_State)
     with Post =>
       State.Initialized
       and then State.Policy_Version = 1
       and then State.Denied_Count = 0;

   --  Register a verified or user package into DysonSphere.
   --  Enforces impersonation protection, rollback prevention, distinct
   --  requested/approved/granted caps, and tier ceilings.
   procedure Register_Package
     (State           : in out Engine_State;
      App_Id          : Bounded_App_Id;
      Publisher       : Bounded_Publisher;
      Version_Code    : Unsigned_32;
      Payload_Hash    : Hash_256;
      Requested_Caps  : Cap_Mask;
      Approved_Caps   : Cap_Mask;
      Source          : Install_Source;
      Is_Verified_V2  : Boolean;
      Platform_Signed : Boolean;
      Decision        : out Auth_Decision)
     with Post =>
       (if not State.Initialized then Decision = Auth_Deny_Policy_Locked)
       and then (if not Is_Valid_App_Id (App_Id) then Decision /= Auth_Allow);

   --  Central deterministic authorization check (Default-DENY).
   procedure Check_Authorization
     (State    : in out Engine_State;
      Request  : Auth_Request;
      Decision : out Auth_Decision)
     with Post =>
       (if not State.Initialized then
          Decision = Auth_Deny_Policy_Locked)
       and then
       (if not Is_Single_Known_Cap (Request.Requested_Cap) then
          Decision /= Auth_Allow)
       and then
       (if not Is_Valid_App_Id (Request.Caller_Id) then
          Decision /= Auth_Allow);

   --  Revoke an application immediately (clears granted capabilities and sets Revoked).
   --  Built-in system identities cannot be revoked through normal user revocation.
   procedure Revoke_App
     (State   : in out Engine_State;
      App_Id  : Bounded_App_Id;
      Revoked : out Boolean)
     with Post =>
       (if Revoked then
          (for all I in App_Slot_Index =>
             (if State.Apps (I).In_Use
                 and then Equal_App_Id (State.Apps (I).App_Id, App_Id)
              then State.Apps (I).Revoked
                   and then State.Apps (I).Granted_Caps = Cap_None)));

   --  Query effective granted capabilities for a registered app (returns 0 if unknown/revoked).
   function Query_Granted_Caps
     (State  : Engine_State;
      App_Id : Bounded_App_Id) return Cap_Mask
     with Post =>
       (if not State.Initialized then Query_Granted_Caps'Result = Cap_None);

end NC_Security_DysonSphere;
