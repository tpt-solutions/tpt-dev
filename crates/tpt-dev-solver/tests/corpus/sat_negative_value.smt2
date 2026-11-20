; expect: sat
(declare-const x Real)
(assert (= x (- 7)))
(check-sat)
