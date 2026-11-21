--  NetizenCore SPARK Ada Security Core — Narrow, Versioned C/Rust ABI
--
--  All security decisions, capability rules, DysonSphere authorization,
--  filesystem policy, cryptography, lock state machine, and /data encryption
--  state transitions are exposed through this stable C-compatible ABI.
--
--  Trust boundary:
--    - The Rust kernel supplies authenticated caller identity (from the kernel
--      task record, never EL0-supplied strings) and valid byte buffer pointers.
--    - All pointer-to-bounded-array marshalling is isolated to the body of this
--      package; every security decision is executed in SPARK_Mode => On units.
--    - Fail-closed: NULL pointers, invalid lengths, ABI version mismatches, or
--      uninitialized engine state always return a DENY / failure code.

pragma SPARK_Mode (Off);

with Interfaces;   use Interfaces;
with Interfaces.C; use Interfaces.C;
with System;

package NC_Security_C_API is

   --  =========================================================================
   --  1. Initialization & ABI Metadata
   --  =========================================================================

   function NC_Sec_ABI_Version return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_abi_version";

   --  Initialize the global SPARK security core (DysonSphere + Lock + FDE).
   --  Must be invoked during early kernel boot before any security decision.
   function NC_Sec_Init (Expected_ABI_Version : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_init";

   function NC_Sec_Is_Initialized return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_is_initialized";

   --  =========================================================================
   --  2. Authoritative Capabilities
   --  =========================================================================

   function NC_Sec_Caps_User_Allowed return Unsigned_64
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_caps_user_allowed";

   function NC_Sec_Caps_System_Only return Unsigned_64
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_caps_system_only";

   function NC_Sec_Caps_Clamp_User (Declared : Unsigned_64) return Unsigned_64
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_caps_clamp_user";

   function NC_Sec_Caps_Dropped_For_User (Declared : Unsigned_64) return Unsigned_64
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_caps_dropped_for_user";

   function NC_Sec_Caps_Is_Valid_Mask (Mask : Unsigned_64) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_caps_is_valid_mask";

   --  =========================================================================
   --  3. DysonSphere Central Authorization Engine
   --  =========================================================================

   function NC_Sec_DS_Authorize
     (Caller_Id_Ptr : System.Address;
      Caller_Id_Len : size_t;
      Requested_Cap : Unsigned_64) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_ds_authorize";

   function NC_Sec_DS_Register_Package
     (App_Id_Ptr      : System.Address;
      App_Id_Len      : size_t;
      Publisher_Ptr   : System.Address;
      Publisher_Len   : size_t;
      Version_Code    : Unsigned_32;
      Payload_Hash    : System.Address;
      Requested_Caps  : Unsigned_64;
      Approved_Caps   : Unsigned_64;
      Source          : Unsigned_32;
      Is_Verified_V2  : Unsigned_32;
      Platform_Signed : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_ds_register_package";

   function NC_Sec_DS_Revoke_App
     (App_Id_Ptr : System.Address;
      App_Id_Len : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_ds_revoke_app";

   function NC_Sec_DS_Query_Granted_Caps
     (App_Id_Ptr : System.Address;
      App_Id_Len : size_t) return Unsigned_64
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_ds_query_granted_caps";

   function NC_Sec_DS_Denied_Count return Unsigned_64
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_ds_denied_count";

   --  Process & IPC policy authorization
   function NC_Sec_Policy_Authorize_Launch
     (Caller_Id_Ptr : System.Address;
      Caller_Id_Len : size_t;
      Caller_Caps   : Unsigned_64;
      Target_Id_Ptr : System.Address;
      Target_Id_Len : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_policy_authorize_launch";

   function NC_Sec_Policy_Authorize_IPC
     (Sender_Id_Ptr : System.Address;
      Sender_Id_Len : size_t;
      Sender_Caps   : Unsigned_64;
      Target_Id_Ptr : System.Address;
      Target_Id_Len : size_t;
      Privileged    : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_policy_authorize_ipc";

   function NC_Sec_Policy_Resolve_Tier
     (Source          : Unsigned_32;
      Is_Verified     : Unsigned_32;
      Platform_Signed : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_policy_resolve_tier";

   --  =========================================================================
   --  4. Authoritative Filesystem Access Policy
   --  =========================================================================

   function NC_Sec_Fs_Check
     (App_Id_Ptr : System.Address;
      App_Id_Len : size_t;
      Caps       : Unsigned_64;
      Path_Ptr   : System.Address;
      Path_Len   : size_t;
      Op         : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fs_check";

   --  =========================================================================
   --  5. Authoritative Cryptographic API
   --  =========================================================================

   procedure NC_Sec_Crypto_Sha256
     (Data_Ptr : System.Address;
      Data_Len : size_t;
      Out_Hash : System.Address)
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_sha256";

   procedure NC_Sec_Crypto_Hmac_Sha256
     (Key_Ptr  : System.Address;
      Key_Len  : size_t;
      Msg_Ptr  : System.Address;
      Msg_Len  : size_t;
      Out_Mac  : System.Address)
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_hmac_sha256";

   function NC_Sec_Crypto_Pbkdf2_Sha256
     (Pass_Ptr   : System.Address;
      Pass_Len   : size_t;
      Salt_Ptr   : System.Address;
      Salt_Len   : size_t;
      Iterations : Unsigned_32;
      Out_Ptr    : System.Address;
      Out_Len    : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_pbkdf2_sha256";

   function NC_Sec_Crypto_Const_Time_Eq
     (A_Ptr : System.Address;
      B_Ptr : System.Address;
      Len   : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_const_time_eq";

   procedure NC_Sec_Crypto_Secure_Zero
     (Buf_Ptr : System.Address;
      Buf_Len : size_t)
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_secure_zero";

   function NC_Sec_Crypto_Aead_Encrypt
     (Key_Ptr    : System.Address;
      Nonce_Ptr  : System.Address;
      Aad_Ptr    : System.Address;
      Aad_Len    : size_t;
      Pt_Ptr     : System.Address;
      Pt_Len     : size_t;
      Ct_Out_Ptr : System.Address;
      Tag_Out    : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_aead_encrypt";

   function NC_Sec_Crypto_Aead_Decrypt
     (Key_Ptr    : System.Address;
      Nonce_Ptr  : System.Address;
      Aad_Ptr    : System.Address;
      Aad_Len    : size_t;
      Ct_Ptr     : System.Address;
      Ct_Len     : size_t;
      Tag_Ptr    : System.Address;
      Pt_Out_Ptr : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_crypto_aead_decrypt";

   --  =========================================================================
   --  6. Authoritative Screen Lock & Credential State Machine
   --  =========================================================================

   function NC_Sec_Lock_Secret_Valid
     (Kind       : Unsigned_8;
      Secret_Ptr : System.Address;
      Secret_Len : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_secret_valid";

   function NC_Sec_Lock_Lockout_Secs (Fails : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_lockout_secs";

   function NC_Sec_Lock_Derive_Sectors
     (Kind            : Unsigned_8;
      Secret_Ptr      : System.Address;
      Secret_Len      : size_t;
      Salt_Ptr        : System.Address;
      Iterations      : Unsigned_32;
      Fails           : Unsigned_32;
      Cred_Sector_Out : System.Address;
      Mark_Sector_Out : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_derive_sectors";

   function NC_Sec_Lock_Evaluate_Sectors
     (Cred_Sector_Ptr : System.Address;
      Mark_Sector_Ptr : System.Address;
      Out_Kind        : access Unsigned_8;
      Out_Iterations  : access Unsigned_32;
      Out_Fails       : access Unsigned_32;
      Out_Salt        : System.Address;
      Out_Hash        : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_evaluate_sectors";

   function NC_Sec_Lock_Verify_Sector
     (Cred_Sector_Ptr : System.Address;
      Secret_Ptr      : System.Address;
      Secret_Len      : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_verify_sector";

   function NC_Sec_Lock_Load_State
     (Cred_Sector_Ptr : System.Address;
      Mark_Sector_Ptr : System.Address;
      Lock_On_Boot    : Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_load_state";

   function NC_Sec_Lock_Is_Locked return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_is_locked";

   function NC_Sec_Lock_Status return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_status";

   function NC_Sec_Lock_Now return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_now";

   function NC_Sec_Lock_Authenticate
     (Secret_Ptr     : System.Address;
      Secret_Len     : size_t;
      Lockout_Active : Unsigned_32;
      Out_Delay_Secs : access Unsigned_32) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_lock_authenticate";

   --  =========================================================================
   --  7. Authoritative Full-Data Encryption (/data) State Machine
   --  =========================================================================

   function NC_Sec_Fde_Load_Metadata
     (Meta_Sector_Ptr : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_load_metadata";

   function NC_Sec_Fde_Provision
     (Pass_Ptr        : System.Address;
      Pass_Len        : size_t;
      Salt_Ptr        : System.Address;
      Nonce_Ptr       : System.Address;
      Raw_Dek_Ptr     : System.Address;
      Iterations      : Unsigned_32;
      Meta_Sector_Out : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_provision";

   function NC_Sec_Fde_Unlock
     (Pass_Ptr : System.Address;
      Pass_Len : size_t) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_unlock";

   procedure NC_Sec_Fde_Lock
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_lock";

   function NC_Sec_Fde_Status return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_status";

   function NC_Sec_Fde_Encrypt_Block
     (Block_Id   : Unsigned_64;
      Nonce_Ptr  : System.Address;
      Pt_Ptr     : System.Address;
      Pt_Len     : size_t;
      Ct_Out_Ptr : System.Address;
      Tag_Out    : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_encrypt_block";

   function NC_Sec_Fde_Decrypt_Block
     (Block_Id   : Unsigned_64;
      Nonce_Ptr  : System.Address;
      Ct_Ptr     : System.Address;
      Ct_Len     : size_t;
      Tag_Ptr    : System.Address;
      Pt_Out_Ptr : System.Address) return Unsigned_32
     with Export        => True,
          Convention    => C,
          External_Name => "nc_sec_fde_decrypt_block";

end NC_Security_C_API;
