--  NetizenCore Zero-Footprint Bare-Metal Runtime Check Trap Hooks
--  Only linked into bare-metal targets (AArch64 / x86_64).
--  If a runtime check ever triggers despite GNATprove verification,
--  execution traps fail-closed via the kernel's `nc_sec_fatal_trap`.

pragma SPARK_Mode (Off);

with Interfaces; use Interfaces;
with System;

package NC_Security_RTS is

   procedure Rcheck_Index
     (File : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_rcheck_CE_Index_Check",
          No_Return;

   procedure Rcheck_Range
     (File : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_rcheck_CE_Range_Check",
          No_Return;

   procedure Rcheck_Overflow
     (File : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_rcheck_CE_Overflow_Check",
          No_Return;

   procedure Rcheck_Invalid_Data
     (File : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_rcheck_CE_Invalid_Data",
          No_Return;

   procedure Rcheck_Length
     (File : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_rcheck_CE_Length_Check",
          No_Return;

   procedure Rcheck_Divide_By_Zero
     (File : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_rcheck_CE_Divide_By_Zero",
          No_Return;

   procedure Last_Chance_Handler
     (Msg : System.Address; Line : Integer)
     with Export        => True,
          Convention    => C,
          External_Name => "__gnat_last_chance_handler",
          No_Return;

   procedure Kernel_Fatal_Trap (Code : Unsigned_32; Line : Integer)
     with Import        => True,
          Convention    => C,
          External_Name => "nc_sec_fatal_trap",
          No_Return;

end NC_Security_RTS;
