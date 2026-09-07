# PE 1006 — Fibonacci Subwords: theory notes (theory seat)

## Setup
Fibonacci word w = fixed point of S0="0", S1="01", S_n = S_{n-1} S_{n-2}
  == fixed point of sigma: 0->01, 1->0.
  w = 0100101001001010010100100101...
For each k there are exactly k+1 distinct length-k factors. Each factor is read
as a DECIMAL number (leading zeros drop by place value, not by a rule):
  int("010") = 10, int("001") = 1.
Psi(k) = sum of squares of those decimal values.

## Anchors (re-verified this turn)
Psi(3)  = 1^2 + 10^2 + 100^2 + 101^2 = 20302        (given, confirmed)
Psi(10) mod 101001001 = 10699667                      (given, confirmed)

## What is NOT the characterization
The k+1 factors are NOT simply the binary strings avoiding "11" and "000".
That set is too big: it already contains non-factors at k=5 ("10101"),
k=6 ("010101","101010"), k=8 ("00100100"), ... The Fibonacci word factor
language is Sturmian (p(n)=n+1) and is NOT finite-state by simple forbidden
patterns; no finite forbidden-pattern list works. So no transfer matrix over
a fixed set of "avoid X" states will be exact.

## Exact structural facts (all verified in theory_state.py)
1. The word is Sturmian: for each k there is exactly ONE right-special factor
   (extends to both '0' and '1'); every other length-k factor extends to exactly
   one letter. The right-special factor is the REVERSE of the prefix w[0..k-1].
   Its decimal value v*(k) satisfies
       v*(0)=0,   v*(k+1) = v*(k) + w[k] * 10^k.

2. The left-special factor (extends by both '0' and '1' on the LEFT) is exactly
   the PREFIX w[0..k-1] (verified k=1..14). Its value P(k):
       P(0)=0,    P(k+1) = 10*P(k) + w[k].

3. Step recurrence for Psi (verified k=1..14):
       Psi(k+1) = 100*( Psi(k) + v*(k)^2 ) + 20*S1(k) + E1(k)
   where
       S1(k) = sum of decimal values of length-k factors that extend right to '1'
       E1(k) = count of length-k factors that extend right to '1'.

4. E1 has an exact closed form (verified k=1..30):
       E1(k) = ceil((k+1)/phi^2),  phi = (1+sqrt5)/2,  phi^2 = phi+1.
   Exact integer version, m = k+1, A = isqrt(5*m*m):
       E1(k) = floor((3*m - A - 1)/2) + 1.
   (phi^2 is irrational so the ceil is well-defined and never hits an integer.)

5. S1 is linked to T1 (sum of decimal values of factors ENDING in '1'):
       T1(k+1) = 10*S1(k) + E1(k),  i.e.  S1(k) = (T1(k+1) - E1(k))/10.

6. T1 recurrence (CORRECTED; the naive T1(k+1)=T1(k)+10^k*L1(k) in the earlier
   notes is WRONG):
       T1(k+1) = T1(k) + 10^k * L1(k+1) + w[k-1] * P(k)
   where
       L1(k) = number of length-k factors that begin AND end with '1'
       (the w[k-1]*P(k) term accounts for the left-special factor, which ends
        in '1' exactly when w[k-1]=1, being counted twice in T1(k+1)).

## BUG FIX (important for @solver / @checker)
analyze.py and psi_verify.py both have a name collision in `state()`:
    e1 = f + "1" in nxt   # bool
    if e1: ...; e1 += 1   # overwrites with an int -> E1 corrupted
This made E1 (and the old "T1 recurrence verified" claim) unreliable. Use
theory_state.py instead; its `state()` returns clean (Psi,S1,T1,vstar,E1,L1,P).

## Remaining open item (handed to @solver)
The only quantity without a closed form yet is L1(k), a Fibonacci-regular
sequence (counts "1...1" factors). Values k=1..50:
 [1,0,1,1,0,2,1,1,3,0,3,2,0,5,1,2,5,0,5,3,1,8,0,5,5,0,9,2,3,9,0,8,5,0,13,
  1,5,10,0,11,5,2,15,0,9,9,0,16,3,5]

Together with w[k] (Fibonacci-automatic bit) the system above closes:
   (Psi, v*, T1, L1, E1, P, w) all computable for k up to 10^18 via
   Fibonacci-regular / Zeckendorf-digit matrix products mod M = 101001001.
M = 101001001 is prime; order of 10 mod M = 50500500 = (M-1)/2 (prior note,
needs re-confirmation if used).

## Next step
@Solver: implement L1(k) for huge k (Zeckendorf/Fibbinary matrix walk), then
iterate the system mod M from k=1 to 10^18 (or jump via Fibonacci blocks) to
produce Psi(10^18) mod M. Verify against Psi(3)=20302 and Psi(10)=10699667 first.
