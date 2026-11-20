; expect: sat
(declare-const x Real)
(declare-const y Real)
(assert (<= x y))
(assert (<= y x))
(assert (= x y))
(check-sat)
