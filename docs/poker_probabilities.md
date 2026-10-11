# A Practical Guide to Poker Probabilities

*From outs and combinations to equity, simulation, and pot odds*

> **The central idea.** First define the possible unseen cards and the event you want to measure. Use combinations when counting unordered deals, the hypergeometric distribution when counting hits among draws without replacement, and expected pot share when calculating equity. Simulation estimates the same quantities when exhaustive enumeration is too expensive.

## Table of contents

1. [Start with the deck and the unknown cards](#1-start-with-the-deck-and-the-unknown-cards)
2. [Combinations: counting unordered choices](#2-combinations-counting-unordered-choices)
3. [Outs and the hypergeometric distribution](#3-outs-and-the-hypergeometric-distribution)
4. [Counting complete poker scenarios](#4-counting-complete-poker-scenarios)
5. [Equity: expected share of the pot](#5-equity-expected-share-of-the-pot)
6. [Monte Carlo equity estimation](#6-monte-carlo-equity-estimation)
7. [Ranges and card removal](#7-ranges-and-card-removal)
8. [Pot odds and the call decision](#8-pot-odds-and-the-call-decision)
9. [Monte Carlo pseudocode](#9-monte-carlo-pseudocode)
10. [Practical implementation checks](#10-practical-implementation-checks)
11. [Summary: which tool answers which question?](#11-summary-which-tool-answers-which-question)

---

## 1. Start with the deck and the unknown cards

A standard deck has 52 cards. Assume the hero's two hole cards are known and the current board has \(b\) cards, where \(b\in\{0,3,4,5\}\) in Texas hold'em. The number of unseen cards is

\[
U=52-2-b=50-b.
\]

The number of community cards still to come is

\[
k=5-b.
\]

| Stage | Board cards \(b\) | Unseen cards \(U\) | Cards to come \(k\) |
|---|---:|---:|---:|
| Preflop | 0 | 50 | 5 |
| Flop | 3 | 47 | 2 |
| Turn | 4 | 46 | 1 |
| River | 5 | 45 | 0 |

The unseen-card count above excludes the hero's cards and the known board, but it does not exclude opponents' unknown cards. Those cards are part of the random deal and must be accounted for when evaluating equity against opponents.

## 2. Combinations: counting unordered choices

If order does not matter, the number of ways to choose \(r\) objects from \(n\) distinct objects is the binomial coefficient

\[
\binom{n}{r}=\frac{n!}{r!(n-r)!}.
\]

For example, an opponent's two-card hand can be chosen from 47 unseen cards in \(\binom{47}{2}=1081\) ways. The two cards \(A\spadesuit,K\spadesuit\) represent one hand, not two different hands, so the order of the cards is not counted.

Use combinations for an unordered hand or an unordered final runout. Use permutations instead if the order of cards is itself part of the event, such as a question specifically about the turn card followed by the river card.

## 3. Outs and the hypergeometric distribution

### Probability of hitting at least one out

Suppose \(K\) of the \(U\) unseen cards are outs: cards that meet a defined target. Draw \(k\) cards without replacement. Let \(X\) be the number of outs drawn. The hypergeometric probability is

\[
\Pr(X=x)=\frac{\binom{K}{x}\binom{U-K}{k-x}}{\binom{U}{k}},
\]

for feasible values

\[
\max(0,k-(U-K))\le x\le\min(K,k).
\]

To find the probability of at least one out, it is simpler to subtract the probability of drawing no outs from one:

\[
\boxed{\Pr(X\ge 1)=1-\frac{\binom{U-K}{k}}{\binom{U}{k}}.}
\]

The numerator in the fraction counts runouts drawn entirely from the \(U-K\) non-outs; the denominator counts all possible \(k\)-card runouts.

### Worked example: a flop flush draw

Suppose the hero has four cards of one suit across the hole cards and flop. There are 13 cards of that suit in the deck, so, under the usual assumptions, \(K=9\) suit cards remain. On the flop, \(U=47\) and \(k=2\). Thus

\[
\begin{aligned}
\Pr(\text{flush by river})
&=1-\frac{\binom{38}{2}}{\binom{47}{2}}\\
&=1-\frac{703}{1081}
=\frac{378}{1081}\approx 0.349676.
\end{aligned}
\]

So the exact probability is approximately **34.97%**. By comparison, the probability of hitting a suit card on the next card alone is

\[
\frac{9}{47}\approx 19.15\%.
\]

> **Important limitation.** This is the chance to hit one of the nine suit cards, not automatically the chance to win. A nominal out can be dirty (for example, it may complete an opponent's full house), and an opponent can redraw even after the hero improves. The hypergeometric formula is exact when the target is a fixed set of \(K\) cards among the unseen cards.

A quick rule of thumb sometimes used at the table is approximately \(2K\%\) for one card to come and \(4K\%\) for two cards to come. It is a rough mental estimate, not a replacement for the exact formula above; for nine outs on the flop, \(4\times9=36\%\), versus the exact \(34.97\%\).

## 4. Counting complete poker scenarios

For equity against opponents, the random scenario must include both the opponents' hole cards and the missing community cards.

Let:

- \(n\) be the number of opponents, with opponents treated as distinguishable players;
- \(U\) be the number of unseen cards after removing the hero's hole cards and the known board;
- \(k\) be the number of board cards still to come.

Choose two cards for the first opponent, two from what remains for the second, and so on. Finally choose the \(k\)-card runout. If the order of cards within each opponent's hand and within the final runout is irrelevant, the number of scenarios is

\[
\begin{aligned}
|\Omega|
&=\left(\prod_{j=0}^{n-1}\binom{U-2j}{2}\right)\binom{U-2n}{k}\\
&=\frac{U!}{(2!)^n\,k!\,(U-2n-k)!}.
\end{aligned}
\]

This requires enough unseen cards: \(2n+k\le U\). The formula counts each opponent's hand assignment separately because the players are distinguishable, but it does not count the order of the two cards in a hand or the order of the final runout.

### Checked examples

**Heads-up on the flop.** There is one opponent, \(U=47\), and \(k=2\):

\[
\begin{aligned}
|\Omega|&=\binom{47}{2}\binom{45}{2}\\
&=1081\times990=\mathbf{1,070,190}.
\end{aligned}
\]

**Heads-up preflop.** There is one opponent, \(U=50\), and \(k=5\):

\[
\begin{aligned}
|\Omega|&=\binom{50}{2}\binom{48}{5}\\
&=1225\times1,712,304=\mathbf{2,097,572,400}.
\end{aligned}
\]

Both values are correct. The board is treated as an unordered five-card set in the preflop count. If the chronological order of the turn and river were relevant, the runout count would instead treat those two cards as ordered; for final-hand equity, their order is irrelevant.

## 5. Equity: expected share of the pot

### Define the payoff for each scenario

A scenario \(\omega\in\Omega\) consists of the opponents' hole cards and the completed board. Let \(r_H(\omega)\) be the hero's final hand rank and \(r_j(\omega)\) the rank of opponent \(j\), with larger ranks defined to mean stronger hands. Put

\[
r_{\max}=\max_{1\le j\le n}r_j,
\qquad
t=\#\{j:r_j=r_{\max}\}.
\]

For a single pot shared equally among the best hands, define the hero's share

\[
s(\omega)=
\begin{cases}
1, & r_H>r_{\max},\\[2pt]
\dfrac{1}{t+1}, & r_H=r_{\max},\\[5pt]
0, & r_H<r_{\max}.
\end{cases}
\]

The middle case applies when the hero ties with \(t\) opponents for the best hand. If the hero wins outright, the share is one; if the hero loses, it is zero.

Under uniformly random legal deals, exact equity is the average share across all scenarios:

\[
\boxed{\operatorname{Equity}=\frac{1}{|\Omega|}\sum_{\omega\in\Omega}s(\omega).}
\]

This definition assumes one pot with equal sharing among tied best hands. Side pots, unequal contributions, or tournament utility effects need a payoff model that reflects those rules.

### Equity is not the same as win probability

It is useful to record three outcome frequencies separately:

- \(P(\text{win})\): the hero finishes strictly ahead of every opponent;
- \(P(\text{tie})\): the hero shares the best rank with at least one opponent;
- \(P(\text{lose})\): at least one opponent finishes ahead of the hero.

These categories sum to one. Equity is the average fraction of the pot received, so ties contribute a fractional amount. In a heads-up pot, equity is \(P(\text{win})+\tfrac12P(\text{tie})\). In a multiway pot, a tie may contribute \(1/3\), \(1/4\), and so forth depending on the number of players tied for best.

## 6. Monte Carlo equity estimation

Exact enumeration can be expensive because the number of possible deals grows rapidly. Monte Carlo simulation instead samples legal scenarios uniformly at random.

Draw \(N\) independent scenarios \(\omega_1,\ldots,\omega_N\), and let \(S_i=s(\omega_i)\in[0,1]\) be the hero's pot share in trial \(i\). The estimator is

\[
\hat e=\frac{1}{N}\sum_{i=1}^N S_i.
\]

It is an unbiased estimate of equity when scenarios are drawn from the correct target distribution.

The estimated unbiased sample variance and standard error are

\[
\begin{aligned}
\hat s^2&=\frac{1}{N-1}\sum_{i=1}^N(S_i-\hat e)^2\\
&=\frac{\sum_{i=1}^N S_i^2-N\hat e^2}{N-1},\\
\widehat{\operatorname{SE}}(\hat e)&=\frac{\hat s}{\sqrt N}.
\end{aligned}
\]

The sample-variance formula requires \(N>1\).

### A standard-error bound and sample size

Because every share lies in \([0,1]\), \(S_i^2\le S_i\). Consequently,

\[
\begin{aligned}
\sum_i(S_i-\hat e)^2
&=\sum_iS_i^2-N\hat e^2\\
&\le N\hat e-N\hat e^2
=N\hat e(1-\hat e).
\end{aligned}
\]

Using the \(N-1\) denominator in the unbiased sample variance gives the bound

\[
\boxed{\widehat{\operatorname{SE}}(\hat e)\le
\sqrt{\frac{\hat e(1-\hat e)}{N-1}}.}
\]

The denominator is \(N-1\), not \(N\), for this particular bound. If one uses the variance with denominator \(N\), the corresponding empirical bound has \(N\) in its denominator.

A common approximate 95% confidence interval is

\[
\hat e\pm1.96\,\widehat{\operatorname{SE}}(\hat e).
\]

This is a normal-approximation interval, not a guarantee of exact 95% coverage. With very small samples or an estimate near 0 or 1, a more carefully constructed interval can be preferable. The same interval can extend a little beyond \([0,1]\); the true equity itself cannot.

For a target half-width \(\varepsilon\), using the worst-case variance \(e(1-e)\le1/4\), the normal-approximation calculation is

\[
1.96\sqrt{\frac{1/4}{N}}\le\varepsilon
\quad\Longrightarrow\quad
N\gtrsim\frac{0.9604}{\varepsilon^2}.
\]

For \(\varepsilon=0.005\) (half-width 0.5 percentage points), this gives

\[
N\gtrsim\frac{0.9604}{0.005^2}=\mathbf{38,416}.
\]

This is a planning approximation; it is not a promise that every run of that size will achieve the requested realized interval width.

## 7. Ranges and card removal

Often an opponent is not assumed to hold every legal hand with equal probability. Instead, each opponent \(j\) has a range weight \(w_j(h_j)\) for each two-card hand \(h_j\). A weight of zero excludes a hand; a weight of one gives it the baseline weight; intermediate values express relative frequency or prior likelihood.

For each legal scenario \(\omega\), use the product of the opponents' hand weights. The range-weighted equity is

\[
\boxed{\operatorname{Equity}_{\mathrm{range}}=
\frac{\displaystyle\sum_{\omega\in\Omega}
\left(\prod_{j=1}^{n}w_j(h_j)\right)s(\omega)}
{\displaystyle\sum_{\omega\in\Omega}
\prod_{j=1}^{n}w_j(h_j)}.}
\]

The denominator normalizes the weights across all legal scenarios. This product form assumes the opponents' ranges are independent priors before enforcing the rule that no two players can hold the same card. Conditioning on card compatibility creates dependencies between the actual dealt hands.

### Correct rejection sampling for ranges

One simple sampler is:

1. Independently draw one hand for each opponent according to that opponent's range weights.
2. If any hand contains a known dead card or any two hands overlap, reject the **entire joint assignment** and draw a fresh set of opponent hands.
3. Once a compatible assignment is accepted, draw the \(k\) board cards uniformly without replacement from the remaining unseen cards.

Rejecting the full joint assignment samples from the product of the stated priors conditioned on card compatibility. Resampling only one conflicting opponent can change the intended joint distribution. In practice, rejection can be inefficient for narrow ranges or many opponents; a legal-combo sampler can be preferable. The range weights and the target joint distribution must be defined consistently.

## 8. Pot odds and the call decision

Let \(P\) be the pot size immediately before the hero calls, including the opponent's bet already in the pot, and let \(C\) be the additional amount the hero must call. If the hero calls, the final pot is \(P+C\). If the hero's equity is \(e\), then the expected net value of calling relative to folding at this decision point is

\[
\begin{aligned}
\operatorname{EV}(\text{call})
&=e(P+C)-C\\
&=eP-(1-e)C.
\end{aligned}
\]

The break-even equity is found by setting this value to zero:

\[
\boxed{e_{\mathrm{BE}}=\frac{C}{P+C}.}
\]

**Example.** Suppose the pot is \(P=100\) before your call, including the opponent's bet, and you must call \(C=50\). Then

\[
e_{\mathrm{BE}}=\frac{50}{100+50}=\frac13\approx33.33\%.
\]

A call is positive in chip EV under this simplified model when your equity exceeds 33.33%. This assumes the equity estimate applies to the opponent's range and the runout, that there are no future betting decisions or additional costs, and that all chips won are valued linearly. Implied odds, reverse implied odds, rake, side pots, and tournament ICM can change a real decision.

## 9. Monte Carlo pseudocode

The following algorithm assumes at least one opponent, uniform random deals from all legal unseen cards, and a hand evaluator for which a larger integer means a stronger hand. It samples all cards for opponents and the runout without replacement.

```text
function EQUITY(hero[2], board, n_opp, N):
    require n_opp >= 1 and N > 1

    dead = hero union board
    Ucards = FULL_DECK \\ dead
    k = 5 - length(board)
    m = 2*n_opp + k
    require m <= length(Ucards)

    sum = 0
    sum_sq = 0
    wins = ties = losses = 0

    repeat N times:
        D = SAMPLE_WITHOUT_REPLACEMENT(Ucards, m)

        for j = 0 .. n_opp-1:
            opp[j] = {D[2*j], D[2*j+1]}
        runout = D[2*n_opp .. m-1]
        full_board = board union runout

        r_hero = RANK(hero union full_board)
        for j = 0 .. n_opp-1:
            r_opp[j] = RANK(opp[j] union full_board)
        r_best = max(r_opp)

        if r_hero > r_best:
            x = 1
            wins += 1
        elif r_hero == r_best:
            t = COUNT(j where r_opp[j] == r_best)
            x = 1 / (t + 1)
            ties += 1
        else:
            x = 0
            losses += 1

        sum += x
        sum_sq += x*x

    equity = sum / N
    variance = (sum_sq - N*equity*equity) / (N - 1)
    variance = max(variance, 0)  # guard against tiny round-off error
    se = sqrt(variance / N)
    return equity, se, wins/N, ties/N, losses/N
```

The assignment of two cards to each labeled opponent and the remaining cards to the board makes every legal joint deal equally likely. The board runout is treated as unordered because only the final five-card board matters for showdown equity. A partial Fisher–Yates shuffle is one efficient way to sample the first \(m\) cards without replacement.

## 10. Practical implementation checks

- **Use a tested hand evaluator.** Evaluating all \(\binom{7}{5}=21\) five-card subsets for every seven-card hand is easy to reason about but costly. A validated evaluator or lookup-table implementation is usually much faster. Ensure that higher rank values consistently mean stronger hands and that equal ranks mean an actual showdown tie.
- **Keep win, tie, and loss counts.** They are nearly free to collect during simulation and make results easier to interpret alongside equity.
- **Prevent card reuse.** Hero cards, known board cards, opponents' hole cards, and the runout must all be mutually disjoint.
- **Test small cases exhaustively.** Compare Monte Carlo estimates with exact enumeration on a manageable flop or turn spot. Check that outcome frequencies sum to one and equity remains in \([0,1]\).
- **Exact enumeration has zero simulation error.** If every legal scenario is evaluated and the payoff rules are correctly modeled, the result is exact for that model. Computation and evaluator correctness can still be sources of implementation error.

## 11. Summary: which tool answers which question?

| Question | Method |
|---|---|
| Will at least one of \(K\) fixed outs arrive in \(k\) cards? | Hypergeometric complement: \(1-\binom{U-K}{k}/\binom{U}{k}\). |
| How many legal opponent hands and runouts exist? | Multiply combinations, or use the complete-scenario formula above. |
| What fraction of the pot should I expect to win at showdown? | Average the pot-share payoff over all legal scenarios, or estimate it by Monte Carlo. |
| How should opponent ranges affect equity? | Weight each legal scenario by the product of the relevant hand weights, then normalize. |
| What equity is needed to call? | Under the simplified chip-EV model, compare equity with \(C/(P+C)\). |

> **A reliable workflow:** (1) define what counts as success, (2) remove known cards, (3) count the relevant combinations without replacement, (4) account for opponents and ties if measuring equity, and (5) state the assumptions behind any estimate or decision rule.

