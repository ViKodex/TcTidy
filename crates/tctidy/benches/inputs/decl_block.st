FUNCTION_BLOCK FB_Conveyor
VAR_INPUT
    Enable          : BOOL;
    TargetSpeed     : LREAL := 0.0;
    Mode            : E_Mode := E_Mode.Idle;
END_VAR
VAR_OUTPUT
    Running         : BOOL;
    ActualSpeed     : LREAL;
    Fault           : BOOL;
    FaultId         : UDINT;
END_VAR
VAR
    rampTimer       : TON;
    state           : INT := 0;
    sensors         : ARRAY[1..8] OF BOOL;
    name            : STRING(80) := 'conveyor';
    pData           : POINTER TO ST_Payload;
    busAddress      AT %IX0.0 : BOOL;
    config          : ST_Config := (speed := 1500, accel := 200, invert := FALSE);
    limits          : ARRAY[0..2] OF LREAL := [10.0, 250.0, 500.0];
END_VAR
