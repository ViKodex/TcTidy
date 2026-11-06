IF Enable AND NOT Fault THEN
    CASE state OF
        0:
            IF TargetSpeed > 0.0 THEN
                state := 1;
            END_IF
        1:
            rampTimer(IN := TRUE, PT := T#2S);
            ActualSpeed := ActualSpeed + (TargetSpeed - ActualSpeed) * 0.1;
            IF rampTimer.Q THEN
                Running := TRUE;
                state := 2;
            END_IF
        2, 3, 4:
            ActualSpeed := TargetSpeed;
        ELSE
            state := 0;
    END_CASE
    FOR i := 1 TO 8 BY 1 DO
        sensors[i] := ReadSensor(index := i, mode := Mode);
    END_FOR
ELSE
    Running := FALSE;
    ActualSpeed := 0.0;
END_IF
