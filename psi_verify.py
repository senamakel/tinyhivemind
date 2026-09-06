#!/usr/bin/env python3
"""PE 1006 — brute-force verification of Psi(k) for small k, plus the state
quantities used in the recurrence (Psi, S1, T1, v*, E1, L1)."""
M = 101001001


def fib_words(n):
    s0, s1 = "0", "01"
    words = [s0, s1]
    for _ in range(2, n):
        words.append(words[-1] + words[-2])
    return words


def state(k, w):
    factors = sorted({w[i:i + k] for i in range(len(w) - k + 1)})
    nxt = {w[i:i + k + 1] for i in range(len(w) - k)}
    vals = [int(f, 10) for f in factors]
    psi = sum(v * v for v in vals)
    s1 = e1 = 0
    vstar = None
    for f in factors:
        e0 = f + "0" in nxt
        e1 = f + "1" in nxt
        if e1:
            s1 += int(f, 10)
            e1 += 1
        if e0 and e1:
            vstar = int(f, 10)
    t1 = sum(int(f, 10) for f in factors if f.endswith("1"))
    l1 = sum(1 for f in factors if f[0] == "1" and f[-1] == "1")
    return psi, s1, t1, vstar, e1, l1


if __name__ == "__main__":
    w = fib_words(26)[-1]  # long enough for k <= ~40
    assert state(3, w)[0] == 20302, "Psi(3) mismatch"
    assert state(10, w)[0] % M == 10699667, "Psi(10) mismatch"
    print("Psi(3) =", state(3, w)[0], " OK")
    print("Psi(10) mod M =", state(10, w)[0] % M, " OK")

    # verify step recurrence Psi(k+1) = 100(Psi(k)+v*^2) + 20*S1(k) + E1(k)
    for k in range(1, 15):
        psi, s1, t1, vs, e1, l1 = state(k, w)
        psi1, *_ = state(k + 1, w)
        pred = 100 * (psi + vs * vs) + 20 * s1 + e1
        assert pred == psi1, f"step recurrence failed at k={k}"
    print("step recurrence verified for k=1..14")

    # verify T1(k+1) = T1(k) + 10^k * L1(k)
    for k in range(1, 15):
        _, _, t1, _, _, l1 = state(k, w)
        _, _, t1n, *_ = state(k + 1, w)
        assert t1n == t1 + 10 ** k * l1, f"T1 recurrence failed at k={k}"
    print("T1 recurrence verified for k=1..14")
