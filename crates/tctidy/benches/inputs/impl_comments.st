(* Conveyor control step.
   Runs the ramp state machine and updates the outputs. *)
IF Enable THEN          // master enable from the HMI
    // ----- ramp phase -----
    state := state + 1; // advance the state counter
    (* The speed filter below is a simple IIR low-pass;
       tune the 0.1 coefficient per machine. *)
    ActualSpeed := ActualSpeed + (TargetSpeed - ActualSpeed) * 0.1; // filter
    // guard against overshoot
    IF ActualSpeed > TargetSpeed THEN
        ActualSpeed := TargetSpeed; // clamp to the setpoint
    END_IF
END_IF
// end of step
