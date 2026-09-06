#!/usr/bin/env python3
"""PE 1006 — clean analysis of Fibonacci word factors and Psi(k) recurrence."""
M = 101001001

def fib_words(n):
    s0, s1 = "0", "01"
    words = [s0, s1]
    for _ in range(2, n):
        words.append(words[-1] + words[-2])
    return words

def state(k, w):
    """Returns (Psi, S1, T1, vstar, E1, L1) for given k."""
    factors = sorted({w[i:i + k] for i in range(len(w) - k + 1)})
    nxt = {w[i:i + k + 1] for i in range(len(w) - k)}
    vals = [int(f, 10) for f in factors]
    psi = sum(v * v for v in vals)

    s1 = 0       # sum of values of factors that extend right to '1'
    e1_ct = 0    # count of factors that extend right to '1'
    vstar = None
    for f in factors:
        ext0 = f + "0" in nxt
        ext1 = f + "1" in nxt
        if ext1:
            s1 += int(f, 10)
            e1_ct += 1
        if ext0 and ext1:
            vstar = int(f, 10)

    t1 = sum(int(f, 10) for f in factors if f.endswith("1"))
    l1 = sum(1 for f in factors if f[0] == "1" and f[-1] == "1")
    return psi, s1, t1, vstar, e1_ct, l1


if __name__ == "__main__":
    w = fib_words(26)[-1]  # |w| = F(27) = 196418, enough for k up to ~40
    assert state(3, w)[0] == 20302, "Psi(3) mismatch"
    assert state(10, w)[0] % M == 10699667, "Psi(10) match"

    print("Psi(3) =", state(3, w)[0], " OK")
    print("Psi(10) mod M =", state(10, w)[0] % M, " OK")

    # print state for k=1..20
    print("\nk  Psi        S1(k)  T1(k)  v*      E1(k) L1(k)")
    for k in range(1, 21):
        psi, s1, t1, vs, e1, l1 = state(k, w)
        print(f"{k:2d} {psi:12d} {s1:8d} {t1:8d} {vs:8d} {e1:5d} {l1:5d}")

    # verify step recurrence: Psi(k+1) = 100(Psi(k)+v*^2) + 20 S1(k) + E1(k)
    print("\n--- Step recurrence checks ---")
    for k in range(1, 16):
        psi, s1, _, vs, e1, _ = state(k, w)
        psi1, *_ = state(k + 1, w)
        pred = 100 * (psi + vs * vs) + 20 * s1 + e1
        ok = "OK" if pred == psi1 else f"MISMATCH pred={pred} actual={psi1}"
        print(f"  k={k}: {ok}")

    # check E1 formula: E1(k) = ceil((k+1)/phi^2)
    phi = (1 + 5 ** 0.5) / 2
    phi2 = phi * phi
    print("\n--- E1 formula check ---")
    for k in range(1, 21):
        _, _, _, _, e1, _ = state(k, w)
        # num 1s in prefix of length k+1 of Fibonacci word
        # the Fibonacci word = floor((n+1)/phi^2) - floor(n/phi^2) for n>=0
        # Prefix sum of 1s up to index n-1 = floor(n/phi^2)
        # Index is 0-based. For length k factors, need ...
        # Actually E1(k) = number of factors ending in '1' (or extending to '1')
        pred_e1 = int((k + 1) / phi2) + 1  # ceil
        ok = "OK" if pred_e1 == e1 else f"MISMATCH pred={pred_e1} actual={e1}"
        print(f"  k={k:2d}: {ok}")

    # check T1 recurrence: T1(k+1) = ?
    print("\n--- T1 recurrence checks ---")
    for k in range(1, 16):
        _, _, t1, _, _, l1 = state(k, w)
        _, _, t1n, _, _, l1n = state(k + 1, w)
        print(f"  k={k:2d}: T1={t1} T1+1={t1n} diff={t1n-t1} L1(k)={l1} L1(k+1)={l1n} 10^k={10**k}")