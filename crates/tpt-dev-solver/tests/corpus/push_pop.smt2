; expect: unsat
; expect: sat
(declare-const x Real)
(assert (>= x 0))
(push)
(assert (< x 0))
(check-sat)
(pop)
(check-sat)
