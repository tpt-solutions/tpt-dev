; Hand-checked QF_LRA corpus. Each `; expect:` line gives the answer to the next check-sat.
; expect: sat
(declare-const x Real)
(assert (>= x 0))
(assert (<= x 1))
(check-sat)
