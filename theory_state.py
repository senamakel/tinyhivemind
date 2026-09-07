#!/usr/bin/env python3
"""PE 1006 — theory: Fibonacci word factor state + verified recurrences.

Fixes the `e1` (bool vs count) name collision present in the earlier
analyze.py / psi_verify.py, which corrupted E1 and silently broke the
"T1(k+1) = T1(k) + 10^k L1(k)" claim (that recurrence is WRONG — see the
corrected form below).

Convention (matches the given anchors Psi(3)=20302, Psi(10) mod M = 10699667):
  w = fixed point of sigma: 0->01, 1->0  ==  S0="0", S1="01", S_n = S_{n-1}S_{n-2}
  w = 0100101001001010010100100101...
Factors of length k are read as BASE-10 integers (leading zeros drop by place
value, not by rule): int("001",10)=1, int("010",10)=10, int("101",10)=101.
"""
from math import isqrt

M = 101001001


def fib_words(n):
    s0, s1 = "0", "01"
    words = [s0, s1]
    for _ in range(2, n):
        words.append(words[-1] + words[-2])
    return words


def state(k, w):
    """Return (Psi, S1, T1, vstar, E1, L1, P) for length-k factors of w.

    Psi  = sum of squares of decimal values of the k+1 factors
    S1   = sum of decimal values of factors that extend right to '1'
    T1   = sum of decimal values of factors that end in '1'
    vstar= decimal value of the unique right-special factor (extends to 0 AND 1)
    E1   = count of factors that extend right to '1'
    L1   = count of factors that begin AND end with '1'
    P    = decimal value of the length-k prefix w[0..k-1]
    """
    factors = sorted({w[i:i + k] for i in range(len(w) - k + 1)})
    nxt = {w[i:i + k + 1] for i in range(len(w) - k)}
    psi = sum(int(f, 10) ** 2 for f in factors)
    s1 = 0
    e1_ct = 0
    vstar = None
    for f in factors:
        ext0 = (f + "0") in nxt
        ext1 = (f + "1") in nxt
        if ext1:
            s1 += int(f, 10)
            e1_ct += 1
        if ext0 and ext1:
            vstar = int(f, 10)
    t1 = sum(int(f, 10) for f in factors if f.endswith("1"))
    l1 = sum(1 for f in factors if f[0] == "1" and f[-1] == "1")
    p = int(w[:k], 10) if k >= 1 else 0
    return psi, s1, t1, vstar, e1_ct, l1, p


def e1_closed(k):
    """E1(k) = ceil((k+1)/phi^2), exact integer arithmetic (no floats).

    With m = k+1, (k+1)/phi^2 = m*(3 - sqrt5)/2. Let A = isqrt(5*m*m) =
    floor(m*sqrt5). Since m*sqrt5 is irrational, floor((3m - m*sqrt5)/2)
    = floor((3m - A - 1)/2), and ceil = that + 1.
    """
    m = k + 1
    A = isqrt(5 * m * m)
    return ((3 * m - A - 1) // 2) + 1


if __name__ == "__main__":
    w = fib_words(26)[-1]  # |w| = F(27) = 196418, enough for k up to ~40

    # Given anchors
    assert state(3, w)[0] == 20302, "Psi(3) mismatch"
    assert state(10, w)[0] % M == 10699667, "Psi(10) mismatch"
    print("Psi(3) = 20302 OK;  Psi(10) mod M = 10699667 OK")

    # Step recurrence: Psi(k+1) = 100(Psi(k)+v*^2) + 20*S1(k) + E1(k)
    for k in range(1, 15):
        psi, s1, t1, vs, e1, l1, p = state(k, w)
        psi1, *_ = state(k + 1, w)
        pred = 100 * (psi + vs * vs) + 20 * s1 + e1
        assert pred == psi1, f"step recurrence failed at k={k}"
    print("step recurrence verified k=1..14")

    # E1 closed form
    for k in range(1, 31):
        assert e1_closed(k) == state(k, w)[4], f"E1 closed form failed at k={k}"
    print("E1(k)=ceil((k+1)/phi^2) closed form verified k=1..30")

    # S1/T1 link: S1(k) = (T1(k+1) - E1(k))/10
    for k in range(1, 15):
        _, s1, t1, _, e1, _, _ = state(k, w)
        _, _, t1n, _, _, _, _ = state(k + 1, w)
        assert s1 == (t1n - e1) // 10, f"S1/T1 link failed at k={k}"
    print("S1(k) = (T1(k+1) - E1(k))/10 verified k=1..14")

    # Left-special factor == prefix w[0..k-1]
    for k in range(1, 15):
        fs = sorted({w[i:i + k] for i in range(len(w) - k + 1)})
        nx = {w[i:i + k + 1] for i in range(len(w) - k)}
        ls = [f for f in fs if ("0" + f) in nx and ("1" + f) in nx]
        assert ls == [w[:k]], f"left-special != prefix at k={k}"
    print("left-special factor == prefix w[0..k-1] verified k=1..14")

    # Corrected T1 recurrence:
    #   T1(k+1) = T1(k) + 10^k * L1(k+1) + w[k-1] * P(k)
    # (the +w[k-1]*P(k) term is the left-special factor counted twice when it
    #  ends in '1'; the naive T1(k)+10^k*L1(k) is WRONG.)
    for k in range(1, 15):
        _, _, t1, _, _, l1, p = state(k, w)
        _, _, t1n, _, _, l1n, _ = state(k + 1, w)
        pred = t1 + 10 ** k * l1n + int(w[k - 1]) * p
        assert pred == t1n, f"corrected T1 recurrence failed at k={k}"
    print("corrected T1 recurrence verified k=1..14")

    # Auxiliary recurrences:
    #   v*(k+1) = v*(k) + w[k]*10^k          (v* = reverse-prefix value)
    #   P(k+1)  = 10*P(k) + w[k]             (P  = prefix value)
    for k in range(1, 15):
        _, _, _, vs, _, _, p = state(k, w)
        _, _, _, vsn, _, _, pn = state(k + 1, w)
        assert vsn == vs + int(w[k]) * 10 ** k, f"v* recurrence failed k={k}"
        assert pn == 10 * p + int(w[k]), f"P recurrence failed k={k}"
    print("v* and P recurrences verified k=1..14")

    # State table k=1..20
    print("\nk  Psi          S1        T1        v*         E1 L1 P")
    for k in range(1, 21):
        psi, s1, t1, vs, e1, l1, p = state(k, w)
        print(f"{k:2d} {psi:12d} {s1:9d} {t1:9d} {vs:9d} {e1:3d} {l1:3d} {p}")

    # L1 sequence k=1..50 (Fibonacci-regular; closed form NOT yet derived)
    wlong = fib_words(30)[-1]  # |w| = F(32) = 2178309
    print("\nL1(k), k=1..50:")
    L1 = []
    for k in range(1, 51):
        seen = set()
        l1 = 0
        for i in range(len(wlong) - k + 1):
            f = wlong[i:i + k]
            if f in seen:
                continue
            seen.add(f)
            if f[0] == "1" and f[-1] == "1":
                l1 += 1
            if len(seen) == k + 1:
                break
        L1.append(l1)
    print(L1)
