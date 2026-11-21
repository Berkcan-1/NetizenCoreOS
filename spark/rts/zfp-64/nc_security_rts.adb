--  NetizenCore Zero-Footprint Bare-Metal Runtime Check Trap Hooks Body

pragma SPARK_Mode (Off);

package body NC_Security_RTS is

   procedure Rcheck_Index (File : System.Address; Line : Integer) is
      pragma Unreferenced (File);
   begin
      Kernel_Fatal_Trap (1, Line);
   end Rcheck_Index;

   procedure Rcheck_Range (File : System.Address; Line : Integer) is
      pragma Unreferenced (File);
   begin
      Kernel_Fatal_Trap (2, Line);
   end Rcheck_Range;

   procedure Rcheck_Overflow (File : System.Address; Line : Integer) is
      pragma Unreferenced (File);
   begin
      Kernel_Fatal_Trap (3, Line);
   end Rcheck_Overflow;

   procedure Rcheck_Invalid_Data (File : System.Address; Line : Integer) is
      pragma Unreferenced (File);
   begin
      Kernel_Fatal_Trap (4, Line);
   end Rcheck_Invalid_Data;

   procedure Rcheck_Length (File : System.Address; Line : Integer) is
      pragma Unreferenced (File);
   begin
      Kernel_Fatal_Trap (5, Line);
   end Rcheck_Length;

   procedure Rcheck_Divide_By_Zero (File : System.Address; Line : Integer) is
      pragma Unreferenced (File);
   begin
      Kernel_Fatal_Trap (6, Line);
   end Rcheck_Divide_By_Zero;

   procedure Last_Chance_Handler (Msg : System.Address; Line : Integer) is
      pragma Unreferenced (Msg);
   begin
      Kernel_Fatal_Trap (99, Line);
   end Last_Chance_Handler;

end NC_Security_RTS;
