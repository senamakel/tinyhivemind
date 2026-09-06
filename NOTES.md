# PE 1006 — Fibonacci Subwords: theory notes

## Setup
Fibonacci word w = fixed point of S0="0", S1="01", S_n = S_{n-1} S_{n-2}.
w = 0100101001001010010100100101...  (also fixed point of sigma: 0->01, 1->0).
For each k there are exactly k+1 distinct length-k factors. Each factor is read
as a DECIMAL number (leading zeros drop by place value, not by a rule):
  int("010") = 10, int("001") = 1.
Psi(k) = sum of squares of those decimal values.

## Verified samples (see psi_verify.py)
Psi(3) = 1^2 + 10^2 + 100^2 + 101^2 = 20302            (given, confirmed)
Psi(10) mod 101001001 = 10699667                        (given, confirmed)

## Factor characterization
For k with F_j <= k < F_{j+1} (Fibonacci F_0=0,F_1=1,F_2=1,...), write k = F_j + r,
0 <= r < F_{j-1}. The k+1 distinct factors are w[n..n+k-1] for n in
  S_k = {0,1,...,F_j-1}  union  {F_{j+1}-1-r, ..., F_{j+1}-1}.
(First-occurrence positions; verified for k=3..13.)

## Decimal mapping of a factor at position n
D(n,k) = sum_{i=0}^{k-1} w[n+i] * 10^{k-1-i}.   (rightmost digit * 10^0)
Window shift:  D(n+1,k) = 10*D(n,k) - w[n]*10^k + w[n+k].

## Step recurrence (going k -> k+1)  [verified numerically k<=20]
Each length-k factor f (decimal value v) extends to a length-(k+1) factor by
appending 0 (v -> 10v) or 1 (v -> 10v+1). Exactly one factor is right-special
(it is the reverse of the length-k prefix of w); it extends both ways.

  Psi(k+1) = 100*(Psi(k) + v*^2) + 20*S1(k) + E1(k)

where
  v*(k) = decimal value of the right-special factor (reverse of w[0..k-1]),
  E1(k) = number of length-k factors that extend to 1,
  S1(k) = sum of decimal values of the length-k factors that extend to 1.

### Auxiliary closed forms (verified)
  v*(k+1) = v*(k) + w[k]*10^k,  v*(0)=0        (prepend w[k] to the reverse)
  Fibonacci jump:  v*(F_{n+2}) = v*(F_n)*10^{F_{n+1}} + v*(F_{n+1}).
  E1(k) = ceil((k+1)/phi^2) = floor((k+1)/phi^2) + 1     (phi = (1+sqrt5)/2)

### The remaining auxiliary: T1 / L1
Let T1(k) = sum of decimal values of length-k factors that END in "1".
A length-(k+1) factor ending in 1 is f·1 for f a length-k factor extending to 1:
  T1(k+1) = 10*S1(k) + E1(k),   and   S1(k) = (T1(k+1) - E1(k))/10.

Alternate (left-extension) view gives
  T1(k+1) = T1(k) + 10^k * L1(k)
where
  L1(k) = number of length-k factors that BEGIN and END with "1".

So the whole system closes once L1(k) is known. L1 is Fibonacci-regular
(counts "1...1" factors of the Fibonacci word). First values:

  L1(k), k=1..30:
  [1,0,1,1,0,2,1,1,3,0,3,2,0,5,1,2,5,0,5,3,1,8,0,5,5,0,9,2,3,9]

## Plan to finish
1. Derive/implement L1(k) via the morphism (Fibonacci-regular recurrence),
   or directly enumerate "1...1" factors.
2. Track the state (Psi, S1, T1, v*, E1, L1) through k=1..10^18 using a
   Fibonacci-automatic matrix product (transition matrix depends on digit w[k]).
3. Report Psi(10^18) mod 101001001.

M = 101001001 is prime; order of 10 mod M = 50500500 = (M-1)/2.
