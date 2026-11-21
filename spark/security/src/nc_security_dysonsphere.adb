--  NetizenCore SPARK Ada Security Core — DysonSphere Central Authorization Engine Body

pragma SPARK_Mode (On);

with NC_Security_Policy; use NC_Security_Policy;

package body NC_Security_DysonSphere is

   function Find_App_Slot
     (State  : Engine_State;
      App_Id : Bounded_App_Id) return Natural
   is
   begin
      for I in App_Slot_Index loop
         pragma Loop_Invariant
           (for all K in 1 .. I - 1 =>
              not (State.Apps (K).In_Use
                   and then Equal_App_Id (State.Apps (K).App_Id, App_Id)));
         if State.Apps (I).In_Use
           and then Equal_App_Id (State.Apps (I).App_Id, App_Id)
         then
            return I;
         end if;
      end loop;
      return 0;
   end Find_App_Slot;

   procedure Record_Audit
     (State    : in out Engine_State;
      App_Id   : Bounded_App_Id;
      Cap_Bits : Cap_Mask;
      Decision : Auth_Decision)
   is
      Tag      : Audit_Tag_Buf := Zero_Audit_Tag;
      Copy_Len : constant Natural :=
        (if App_Id.Len < 24 then App_Id.Len else 24);
   begin
      for I in 0 .. Copy_Len - 1 loop
         Tag (I) := App_Id.Data (I + 1);
      end loop;

      State.Audit_Seq := State.Audit_Seq + 1;
      State.Audit (State.Audit_Write_Idx) :=
        (Seq           => State.Audit_Seq,
         App_Id_Tag    => Tag,
         Requested_Cap => Cap_Bits,
         Decision      => Decision);
      State.Audit_Write_Idx := (State.Audit_Write_Idx + 1) mod Audit_Ring_Capacity;
   end Record_Audit;

   procedure Register_Builtin
     (State        : in out Engine_State;
      App_Id_Str   : String;
      Granted_Caps : Cap_Mask)
     with Pre => App_Id_Str'Length <= Max_App_Id_Len
   is
      Id : constant Bounded_App_Id := Make_App_Id (App_Id_Str);
      Free_Slot : Natural := 0;
   begin
      for I in App_Slot_Index loop
         if not State.Apps (I).In_Use then
            Free_Slot := I;
            exit;
         end if;
      end loop;
      if Free_Slot in App_Slot_Index then
         State.Apps (Free_Slot) :=
           (In_Use              => True,
            App_Id              => Id,
            Publisher           => Make_Publisher ("system:builtin"),
            Version_Code        => 1,
            Min_Allowed_Version => 1,
            Payload_Hash        => Zero_Hash_256,
            Requested_Caps      => Granted_Caps and All_Known_Caps,
            Approved_Caps       => Granted_Caps and All_Known_Caps,
            Granted_Caps        => Granted_Caps and All_Known_Caps,
            Tier                => Tier_System,
            Revoked             => False,
            Is_Builtin          => True);
         if State.App_Count < Max_Registered_Apps then
            State.App_Count := State.App_Count + 1;
         end if;
      end if;
   end Register_Builtin;

   procedure Initialize (State : out Engine_State) is
   begin
      State :=
        (Initialized     => True,
         Policy_Version  => 1,
         Apps            => (others => <>),
         App_Count       => 0,
         Audit           => (others => <>),
         Audit_Write_Idx => 0,
         Audit_Seq       => 0,
         Denied_Count    => 0);

      --  Least-privilege built-in system application policies.
      --  1. NetizenHome (nc.home): launcher + read settings + post notifications
      Register_Builtin
        (State,
         "nc.home",
         Cap_Settings_Read
         or Cap_Process_Launch
         or Cap_Ipc_Endpoint
         or Cap_Notify_Post);

      --  2. Settings (nc.settings): settings + security management + app inspection
      Register_Builtin
        (State,
         "nc.settings",
         Cap_Settings_Read
         or Cap_Settings_Write_Display
         or Cap_Settings_Write_Locale
         or Cap_Settings_Write_Keybd
         or Cap_Settings_Write_Network
         or Cap_Settings_Write_Drivers
         or Cap_Security_Manage
         or Cap_Package_Remove
         or Cap_Process_Launch);

      --  3. Files (nc.files): user & device storage + package install handoff
      Register_Builtin
        (State,
         "nc.files",
         Cap_Fs_User_Read
         or Cap_Fs_User_Write
         or Cap_Fs_User_Delete
         or Cap_Fs_All_Read
         or Cap_Fs_All_Write
         or Cap_Package_Install);

      --  4. Package Installer (nc.package-installer): package install/remove + read storage
      Register_Builtin
        (State,
         "nc.package-installer",
         Cap_Fs_User_Read
         or Cap_Fs_All_Read
         or Cap_Package_Install
         or Cap_Package_Remove);

      --  5. Browser (nc.browser): network client + user file read
      Register_Builtin
        (State,
         "nc.browser",
         Cap_Network_Client or Cap_Fs_User_Read);

      --  6. System Monitor (nc.system): read settings + read security audit
      Register_Builtin
        (State,
         "nc.system",
         Cap_Settings_Read or Cap_System_Audit_Read);

      --  7. Stopwatch (nc.stopwatch): zero sensitive capabilities
      Register_Builtin (State, "nc.stopwatch", Cap_None);

      --  8..13. Mobile built-in system applications: strict least-privilege
      --         (settings.read only; sensitive resources require runtime permissions).
      Register_Builtin (State, "nc.keyboard", Cap_Settings_Read);
      Register_Builtin (State, "nc.gallery",  Cap_Settings_Read);
      Register_Builtin (State, "nc.messages", Cap_Settings_Read);
      Register_Builtin (State, "nc.phone",    Cap_Settings_Read);
      Register_Builtin (State, "nc.contacts", Cap_Settings_Read);
      Register_Builtin (State, "nc.camera",   Cap_Settings_Read);

      State.Initialized    := True;
      State.Policy_Version := 1;
      State.Denied_Count   := 0;
   end Initialize;

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
   is
      Tier         : Trust_Tier;
      Ceiling      : Cap_Mask;
      Effective    : Cap_Mask;
      Existing_Idx : Natural;
      Free_Slot    : Natural := 0;
   begin
      if not State.Initialized then
         Decision := Auth_Deny_Policy_Locked;
         return;
      end if;

      if not Is_Valid_App_Id (App_Id) or else Version_Code = 0 then
         Record_Audit (State, App_Id, Requested_Caps, Auth_Deny_Invalid_Input);
         Decision := Auth_Deny_Invalid_Input;
         return;
      end if;

      --  Prevent third-party impersonation of reserved system identities.
      Tier := Resolve_Trust_Tier (Source, Is_Verified_V2, Platform_Signed);
      if Is_Reserved_System_Id (App_Id) and then Tier /= Tier_System then
         Record_Audit (State, App_Id, Requested_Caps, Auth_Deny_Impersonation);
         Decision := Auth_Deny_Impersonation;
         return;
      end if;

      --  Compute policy ceiling:
      --    - System tier (SystemImage + verified + platform-signed): All_Known_Caps
      --    - Verified third-party: Third_Party_Verified_Ceiling
      --    - Unverified / free package: Cap_None (full sandbox unless explicitly user-approved within user mask)
      if Tier = Tier_System then
         Ceiling := All_Known_Caps;
      elsif Is_Verified_V2 then
         Ceiling := Third_Party_Verified_Ceiling;
      else
         Ceiling := Cap_None;
      end if;

      Effective := Compute_Effective_Grant (Requested_Caps, Approved_Caps, Ceiling);

      Existing_Idx := Find_App_Slot (State, App_Id);
      if Existing_Idx in App_Slot_Index then
         --  Revoked app remains revoked (cannot silently un-revoke via reinstall).
         if State.Apps (Existing_Idx).Revoked then
            Record_Audit (State, App_Id, Effective, Auth_Deny_Revoked_App);
            Decision := Auth_Deny_Revoked_App;
            return;
         end if;

         --  Rollback prevention: version must never drop below min_allowed_version.
         if Version_Code < State.Apps (Existing_Idx).Min_Allowed_Version then
            Record_Audit (State, App_Id, Effective, Auth_Deny_Rollback_Version);
            Decision := Auth_Deny_Rollback_Version;
            return;
         end if;

         --  Update cannot silently acquire privileges beyond previous grant + explicit approval.
         State.Apps (Existing_Idx).Publisher           := Publisher;
         State.Apps (Existing_Idx).Version_Code        := Version_Code;
         State.Apps (Existing_Idx).Min_Allowed_Version := Version_Code;
         State.Apps (Existing_Idx).Payload_Hash        := Payload_Hash;
         State.Apps (Existing_Idx).Requested_Caps      := Requested_Caps and All_Known_Caps;
         State.Apps (Existing_Idx).Approved_Caps       := Approved_Caps and All_Known_Caps;
         State.Apps (Existing_Idx).Granted_Caps        := Effective;
         State.Apps (Existing_Idx).Tier                := Tier;
         Decision := Auth_Allow;
         return;
      end if;

      for I in App_Slot_Index loop
         if not State.Apps (I).In_Use then
            Free_Slot := I;
            exit;
         end if;
      end loop;

      if Free_Slot not in App_Slot_Index then
         Record_Audit (State, App_Id, Effective, Auth_Deny_Table_Full);
         Decision := Auth_Deny_Table_Full;
         return;
      end if;

      State.Apps (Free_Slot) :=
        (In_Use              => True,
         App_Id              => App_Id,
         Publisher           => Publisher,
         Version_Code        => Version_Code,
         Min_Allowed_Version => Version_Code,
         Payload_Hash        => Payload_Hash,
         Requested_Caps      => Requested_Caps and All_Known_Caps,
         Approved_Caps       => Approved_Caps and All_Known_Caps,
         Granted_Caps        => Effective,
         Tier                => Tier,
         Revoked             => False,
         Is_Builtin          => False);
      if State.App_Count < Max_Registered_Apps then
         State.App_Count := State.App_Count + 1;
      end if;
      Decision := Auth_Allow;
   end Register_Package;

   procedure Check_Authorization
     (State    : in out Engine_State;
      Request  : Auth_Request;
      Decision : out Auth_Decision)
   is
      Slot : Natural;
   begin
      if not State.Initialized then
         Decision := Auth_Deny_Policy_Locked;
         return;
      end if;

      if not Is_Valid_App_Id (Request.Caller_Id)
        or else not Is_Single_Known_Cap (Request.Requested_Cap)
      then
         Decision := Auth_Deny_Invalid_Input;
         Record_Audit (State, Request.Caller_Id, Request.Requested_Cap, Decision);
         if State.Denied_Count < Unsigned_64'Last then
            State.Denied_Count := State.Denied_Count + 1;
         end if;
         return;
      end if;

      --  Direct kernel-internal identity (authenticated by kernel, never EL0).
      if Equal_App_Id (Request.Caller_Id, Make_App_Id ("nc.kernel")) then
         Decision := Auth_Allow;
         return;
      end if;

      Slot := Find_App_Slot (State, Request.Caller_Id);
      if Slot not in App_Slot_Index then
         Decision := Auth_Deny_Unknown_App;
      elsif State.Apps (Slot).Revoked then
         Decision := Auth_Deny_Revoked_App;
      elsif State.Apps (Slot).Version_Code < State.Apps (Slot).Min_Allowed_Version then
         Decision := Auth_Deny_Rollback_Version;
      elsif Has_Capability (State.Apps (Slot).Granted_Caps, Request.Requested_Cap) then
         Decision := Auth_Allow;
      else
         Decision := Auth_Deny_Capability_Not_Granted;
      end if;

      Record_Audit (State, Request.Caller_Id, Request.Requested_Cap, Decision);
      if Decision /= Auth_Allow and then State.Denied_Count < Unsigned_64'Last then
         State.Denied_Count := State.Denied_Count + 1;
      end if;
   end Check_Authorization;

   procedure Revoke_App
     (State   : in out Engine_State;
      App_Id  : Bounded_App_Id;
      Revoked : out Boolean)
   is
      Slot : Natural;
   begin
      Revoked := False;
      if not State.Initialized or else not Is_Valid_App_Id (App_Id) then
         return;
      end if;

      Slot := Find_App_Slot (State, App_Id);
      if Slot in App_Slot_Index and then not State.Apps (Slot).Is_Builtin then
         State.Apps (Slot).Revoked      := True;
         State.Apps (Slot).Granted_Caps := Cap_None;
         Revoked := True;
         --  Ensure uniqueness invariant holds for postcondition:
         for I in App_Slot_Index loop
            pragma Loop_Invariant
              (for all K in 1 .. I - 1 =>
                 (if State.Apps (K).In_Use
                     and then Equal_App_Id (State.Apps (K).App_Id, App_Id)
                  then State.Apps (K).Revoked
                       and then State.Apps (K).Granted_Caps = Cap_None));
            if State.Apps (I).In_Use
              and then Equal_App_Id (State.Apps (I).App_Id, App_Id)
            then
               State.Apps (I).Revoked      := True;
               State.Apps (I).Granted_Caps := Cap_None;
            end if;
         end loop;
         Record_Audit (State, App_Id, Cap_None, Auth_Deny_Revoked_App);
      end if;
   end Revoke_App;

   function Query_Granted_Caps
     (State  : Engine_State;
      App_Id : Bounded_App_Id) return Cap_Mask
   is
      Slot : Natural;
   begin
      if not State.Initialized then
         return Cap_None;
      end if;
      Slot := Find_App_Slot (State, App_Id);
      if Slot not in App_Slot_Index then
         return Cap_None;
      end if;
      if State.Apps (Slot).Revoked
        or else State.Apps (Slot).Version_Code < State.Apps (Slot).Min_Allowed_Version
      then
         return Cap_None;
      end if;
      return State.Apps (Slot).Granted_Caps;
   end Query_Granted_Caps;

end NC_Security_DysonSphere;
