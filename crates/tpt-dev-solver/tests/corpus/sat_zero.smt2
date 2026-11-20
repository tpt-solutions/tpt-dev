; expect: sat
(declare-const x Real)
(assert (= x 0))
(check-sat)
