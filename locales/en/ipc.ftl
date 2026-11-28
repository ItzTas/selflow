ipc-not-locked = Nothing is locked right now, so there's no extra time to ask for.
ipc-failed = Selflow couldn't finish that just now. Try again in a moment.
ipc-test-lock-real-power = Test locks only work while Selflow runs in test mode. Start the daemon with SELFLOW_POWER=log to try one.
ipc-test-lock-duration = A test lock can last from { $min } to { $max } minutes.
ipc-test-lock-busy = A lock is on screen right now. You can start a test lock once it ends.
ipc-test-lock-no-rule = There's no lock rule for today. Pick one with --rule.
ipc-test-lock-unknown-rule = There's no rule called { $rule }. The rules you have are: { $rules }
ipc-release-locked = A lock is on screen now, so it stays. To ask for more time, run selflow unlock again.
ipc-exception-unknown-rule = There's no rule called { $rule } today. Today's rules are: { $rules }
ipc-exception-locked = The { $label } lock is running tonight, so it can't be skipped.
ipc-exception-done = The { $label } rule is already off for the rest of the day.
ipc-exception-not-started = There's no request to skip this rule today. Run selflow exception again to start one.
