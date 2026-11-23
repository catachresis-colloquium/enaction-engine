-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
--
-- Independent mechanisation under ADR-0025: written on 2026-11-17 by a
-- writer that was given only the prose contract and the seven accepted
-- input packages of conformance/tom/v1, and was refused the Rust reader,
-- every expected.json and the Rocq mechanisation.
{-# OPTIONS --safe --without-K #-}

-- Mechanised reading of CAC-KERNEL section 9, "Models are compared with
-- minds" (with ADR-0028 and ADR-0029), written from the prose alone.
--
-- Every definition is a computable function; the predicted outputs are
-- obtained by normalising `models` (see TomJson.agda / Predicted.agda).

module TomSpec where

open import Agda.Builtin.String using (String; primStringEquality)
open import Data.Bool.Base using (Bool; true; false; _∧_; _∨_; not; if_then_else_)
open import Data.Nat.Base
  using (ℕ; zero; suc; _≡ᵇ_; _<_; _≤_; _∸_; _⊔_; _⊓_; s≤s; z≤n)
open import Data.Nat.Properties
  using (≤-trans; <-trans; n<1+n; m≤n⇒m∸n≡0; n≤1+n; m≤m⊔n; m≤n⊔m)
open import Data.List.Base using (List; []; _∷_; map)
open import Data.Maybe.Base using (Maybe; just; nothing)
open import Data.Product.Base using (_×_; _,_; proj₁; Σ-syntax)
open import Data.Unit.Base using (⊤; tt)
open import Relation.Binary.PropositionalEquality
  using (_≡_; _≢_; refl; sym; trans; cong; cong₂)

------------------------------------------------------------------------
-- 1. Data model (the fields of section 3 that the rule reads)

data Status : Set where
  certain believed suspected rejected unknown : Status

record Belief : Set where
  constructor mkBelief
  field
    beliefId    : ℕ
    holders     : List String   -- ascription path, outermost first
    bearer      : String
    proposition : String
    status      : Status
    validFrom   : ℕ
    supersedes  : Maybe ℕ
open Belief public

Package : Set
Package = List Belief

------------------------------------------------------------------------
-- 2. Small equalities

_==ˢ_ : String → String → Bool
_==ˢ_ = primStringEquality

_==ᵖ_ : List String → List String → Bool
[]       ==ᵖ []       = true
(x ∷ xs) ==ᵖ (y ∷ ys) = (x ==ˢ y) ∧ (xs ==ᵖ ys)
_        ==ᵖ _        = false

-- Structural ≤ on ticks (the library one is defined through <ᵇ, which
-- makes the proofs below needlessly indirect).
infix 4 _≤ᵇ_
_≤ᵇ_ : ℕ → ℕ → Bool
zero  ≤ᵇ _     = true
suc a ≤ᵇ zero  = false
suc a ≤ᵇ suc b = a ≤ᵇ b

filterᵇ : ∀ {A : Set} → (A → Bool) → List A → List A
filterᵇ f []       = []
filterᵇ f (x ∷ xs) = if f x then x ∷ filterᵇ f xs else filterᵇ f xs

any : ∀ {A : Set} → (A → Bool) → List A → Bool
any f []       = false
any f (x ∷ xs) = f x ∨ any f xs

eqᵇ : Bool → Bool → Bool
eqᵇ true  true  = true
eqᵇ false false = true
eqᵇ _     _     = false

------------------------------------------------------------------------
-- 3. "Holds" (the proposition) and "the claim"

-- "A belief holds its proposition when its status is certain, believed or
-- suspected."
holdsProp : Status → Bool
holdsProp certain   = true
holdsProp believed  = true
holdsProp suspected = true
holdsProp rejected  = false
holdsProp unknown   = false

-- "The claim": just true = says the modelled belief holds; just false =
-- says it does not; nothing = no claim.
claim : Status → Maybe Bool
claim certain   = just true
claim believed  = just true
claim suspected = just true
claim rejected  = just false
claim unknown   = nothing

------------------------------------------------------------------------
-- 4. Holding at a tick, and `until`

names : ℕ → Maybe ℕ → Bool
names i nothing  = false
names i (just j) = j ≡ᵇ i

-- Every belief in the package that names `b` in `supersedes`, whatever its
-- own bearer, path or proposition.
superseders : Package → Belief → List Belief
superseders p b = filterᵇ (λ c → names (beliefId b) (supersedes c)) p

startedBy : ℕ → Belief → Bool
startedBy t c = validFrom c ≤ᵇ t

-- "A belief holds at tick t when its valid_from is at most t and no belief
-- in the package that names it in supersedes has a valid_from of at most t."
holdsAt : Package → Belief → ℕ → Bool
holdsAt p b t = startedBy t b ∧ not (any (startedBy t) (superseders p b))

leastFrom : ℕ → List ℕ → ℕ
leastFrom x []       = x
leastFrom x (y ∷ ys) = leastFrom (x ⊓ y) ys

least : List ℕ → Maybe ℕ
least []       = nothing
least (x ∷ xs) = just (leastFrom x xs)

-- "until, the earliest valid_from of a belief that supersedes the model";
-- nothing when nothing supersedes it.
until : Package → Belief → Maybe ℕ
until p b = least (map validFrom (superseders p b))

------------------------------------------------------------------------
-- 5. Models and the compared kind

isModel : Belief → Bool
isModel b with holders b
... | []    = false
... | _ ∷ _ = true

-- The outermost holder removed.
comparedPath : List String → List String
comparedPath []       = []
comparedPath (_ ∷ hs) = hs

-- "holders = [h2, …, hn], bearer = b, the same proposition".
ofComparedKind : Belief → Belief → Bool
ofComparedKind m c =
  (holders c ==ᵖ comparedPath (holders m))
  ∧ (bearer c ==ˢ bearer m)
  ∧ (proposition c ==ˢ proposition m)

-- The beliefs of the compared kind that hold at tick t.
counterpartsAt : Package → Belief → ℕ → List Belief
counterpartsAt p m t = filterᵇ (λ c → holdsAt p c t) (filterᵇ (ofComparedKind m) p)

-- Does the modelled mind hold the proposition at t?
--   none holds  -> it does not (closed over stated models)
--   exactly one -> by that belief's status
--   several     -> undecidable (nothing)
mindHolds : List Belief → Maybe Bool
mindHolds []          = just false
mindHolds (c ∷ [])    = just (holdsProp (status c))
mindHolds (_ ∷ _ ∷ _) = nothing

------------------------------------------------------------------------
-- 6. The verdict at a tick

data Verdict : Set where
  agrees diverges noClaim : Verdict

-- nothing = the rules cannot decide (model-counterpart-unique).
-- A model that makes no claim "is compared with nothing", so it is
-- no-claim before the counterparts are even looked at.
judge : Maybe Bool → Maybe Bool → Maybe Verdict
judge nothing     _           = just noClaim
judge (just _)    nothing     = nothing
judge (just says) (just does) = just (if eqᵇ says does then agrees else diverges)

verdictAt : Package → Belief → ℕ → Maybe Verdict
verdictAt p m t = judge (claim (status m)) (mindHolds (counterpartsAt p m t))

------------------------------------------------------------------------
-- 7. The lifetime and the timeline

record Segment : Set where
  constructor seg
  field
    fromTick : ℕ
    verdict  : Verdict
open Segment public

data Result : Set where
  refused : ℕ → Result              -- model-counterpart-unique at this tick
  ok      : List Segment → Result

-- Latest valid_from in the package.  No belief starts or stops holding
-- after it, so it stands in for "the end of the package".
horizon : Package → ℕ
horizon []      = 0
horizon (b ∷ p) = validFrom b ⊔ horizon p

-- First tick that is no longer in the lifetime.
endExclusive : Maybe ℕ → ℕ → ℕ
endExclusive (just u) h = u
endExclusive nothing  h = suc h

upFrom : ℕ → ℕ → List ℕ
upFrom s zero    = []
upFrom s (suc n) = s ∷ upFrom (suc s) n

-- The ticks of the lifetime after valid_from:
--   valid_from + 1 .. until - 1            (superseded)
--   valid_from + 1 .. horizon              (not superseded)
laterTicks : Package → Belief → List ℕ
laterTicks p m =
  upFrom (suc (validFrom m))
         (endExclusive (until p m) (horizon p) ∸ suc (validFrom m))

data Sampled : Set where
  refusedAt : ℕ → Sampled
  sampled   : List (ℕ × Verdict) → Sampled

consSample : ℕ → Maybe Verdict → Sampled → Sampled
consSample t nothing  _             = refusedAt t   -- the first such tick
consSample t (just v) (refusedAt u) = refusedAt u
consSample t (just v) (sampled xs)  = sampled ((t , v) ∷ xs)

-- "A model is compared at every tick of its lifetime".
sample : (ℕ → Maybe Verdict) → List ℕ → Sampled
sample f []       = sampled []
sample f (t ∷ ts) = consSample t (f t) (sample f ts)

_==ᵛ_ : Verdict → Verdict → Bool
agrees   ==ᵛ agrees   = true
diverges ==ᵛ diverges = true
noClaim  ==ᵛ noClaim  = true
_        ==ᵛ _        = false

-- "A new segment starts at each later tick ... where the verdict changes."
compress : Verdict → List (ℕ × Verdict) → List Segment
compress prev []             = []
compress prev ((t , v) ∷ xs) =
  if v ==ᵛ prev then compress prev xs else seg t v ∷ compress v xs

-- "The first segment starts at the model's valid_from."
build : ℕ → Maybe Verdict → Sampled → Result
build vf nothing  _             = refused vf
build vf (just v) (refusedAt t) = refused t
build vf (just v) (sampled xs)  = ok (seg vf v ∷ compress v xs)

timeline : Package → Belief → Result
timeline p m =
  build (validFrom m)
        (verdictAt p m (validFrom m))
        (sample (verdictAt p m) (laterTicks p m))

------------------------------------------------------------------------
-- 8. Stale divergence

isAgrees : Verdict → Bool
isAgrees agrees = true
isAgrees _      = false

isDiverges : Verdict → Bool
isDiverges diverges = true
isDiverges _        = false

-- "its first segment agrees and a later segment diverges"
staleDivergent : List Segment → Bool
staleDivergent []       = false
staleDivergent (s ∷ ss) = isAgrees (verdict s) ∧ any (λ x → isDiverges (verdict x)) ss

------------------------------------------------------------------------
-- 9. The reader's output: every model, in belief-id order

data ModelOut : Set where
  derived   : (id : ℕ) (segments : List Segment) (until′ : Maybe ℕ)
              (stale : Bool) → ModelOut
  undecided : (id : ℕ) (tick : ℕ) → ModelOut

finish : ℕ → Maybe ℕ → Result → ModelOut
finish i u (ok segs)   = derived i segs u (staleDivergent segs)
finish i u (refused t) = undecided i t

derive : Package → Belief → ModelOut
derive p m = finish (beliefId m) (until p m) (timeline p m)

insertById : Belief → List Belief → List Belief
insertById b []       = b ∷ []
insertById b (c ∷ cs) =
  if beliefId b ≤ᵇ beliefId c then b ∷ c ∷ cs else c ∷ insertById b cs

sortById : List Belief → List Belief
sortById []       = []
sortById (b ∷ bs) = insertById b (sortById bs)

models : Package → List ModelOut
models p = map (derive p) (sortById (filterᵇ isModel p))

------------------------------------------------------------------------
-- 10. Sanity lemmas that the prose implies

-- Strictly increasing, and strictly above a lower bound.
data Inc : ℕ → List ℕ → Set where
  []  : ∀ {lo} → Inc lo []
  _∷_ : ∀ {lo t ts} → lo < t → Inc t ts → Inc lo (t ∷ ts)

StrictlyIncreasing : List ℕ → Set
StrictlyIncreasing []       = ⊤
StrictlyIncreasing (t ∷ ts) = Inc t ts

-- Each element differs from the one before it.
data Alt : Verdict → List Verdict → Set where
  []  : ∀ {v} → Alt v []
  _∷_ : ∀ {v w ws} → w ≢ v → Alt w ws → Alt v (w ∷ ws)

ConsecutiveDiffer : List Verdict → Set
ConsecutiveDiffer []       = ⊤
ConsecutiveDiffer (v ∷ vs) = Alt v vs

private
  Inc-weaken : ∀ {lo lo′ ts} → lo < lo′ → Inc lo′ ts → Inc lo ts
  Inc-weaken lt []            = []
  Inc-weaken lt (lt′ ∷ rest) = <-trans lt lt′ ∷ rest

  upFrom-inc : ∀ {lo} s n → lo < s → Inc lo (upFrom s n)
  upFrom-inc s zero    lt = []
  upFrom-inc s (suc n) lt = lt ∷ upFrom-inc (suc s) n (n<1+n s)

  sample-inc : ∀ f {lo} ts {xs} → Inc lo ts → sample f ts ≡ sampled xs →
               Inc lo (map proj₁ xs)
  sample-inc f []       []         refl = []
  sample-inc f (t ∷ ts) (lt ∷ inc) eq with f t | sample f ts in eq′
  sample-inc f (t ∷ ts) (lt ∷ inc) ()   | nothing | _
  sample-inc f (t ∷ ts) (lt ∷ inc) ()   | just v  | refusedAt u
  sample-inc f (t ∷ ts) (lt ∷ inc) refl | just v  | sampled ys =
    lt ∷ sample-inc f ts inc eq′

  compress-inc : ∀ prev {lo} xs → Inc lo (map proj₁ xs) →
                 Inc lo (map fromTick (compress prev xs))
  compress-inc prev []             []         = []
  compress-inc prev ((t , v) ∷ xs) (lt ∷ inc) with v ==ᵛ prev
  ... | true  = Inc-weaken lt (compress-inc prev xs inc)
  ... | false = lt ∷ compress-inc v xs inc

  ==ᵛ-false : ∀ v w → (v ==ᵛ w) ≡ false → v ≢ w
  ==ᵛ-false agrees   agrees   ()
  ==ᵛ-false diverges diverges ()
  ==ᵛ-false noClaim  noClaim  ()
  ==ᵛ-false agrees   diverges _ ()
  ==ᵛ-false agrees   noClaim  _ ()
  ==ᵛ-false diverges agrees   _ ()
  ==ᵛ-false diverges noClaim  _ ()
  ==ᵛ-false noClaim  agrees   _ ()
  ==ᵛ-false noClaim  diverges _ ()

  compress-alt : ∀ prev xs → Alt prev (map verdict (compress prev xs))
  compress-alt prev []             = []
  compress-alt prev ((t , v) ∷ xs) with v ==ᵛ prev in eq
  ... | true  = compress-alt prev xs
  ... | false = ==ᵛ-false v prev eq ∷ compress-alt v xs

  -- What an accepted timeline looks like.
  build-inv : ∀ vf mv s {segs} → build vf mv s ≡ ok segs →
              Σ[ v ∈ Verdict ] Σ[ xs ∈ List (ℕ × Verdict) ]
                (mv ≡ just v × s ≡ sampled xs × segs ≡ seg vf v ∷ compress v xs)
  build-inv vf nothing  s             ()
  build-inv vf (just v) (refusedAt t) ()
  build-inv vf (just v) (sampled xs)  refl = v , xs , refl , refl , refl

-- (a) "The first segment starts at the model's valid_from": a derived
--     timeline is never empty and its first segment is at valid_from.
timeline-starts-at-validFrom :
  ∀ p m {segs} → timeline p m ≡ ok segs →
  Σ[ v ∈ Verdict ] Σ[ rest ∈ List Segment ] segs ≡ seg (validFrom m) v ∷ rest
timeline-starts-at-validFrom p m eq
  with build-inv (validFrom m) (verdictAt p m (validFrom m))
                 (sample (verdictAt p m) (laterTicks p m)) eq
... | v , xs , _ , _ , refl = v , compress v xs , refl

-- (b) "the ordered list of segments": from_tick is strictly increasing.
timeline-ticks-increase :
  ∀ p m {segs} → timeline p m ≡ ok segs →
  StrictlyIncreasing (map fromTick segs)
timeline-ticks-increase p m eq
  with build-inv (validFrom m) (verdictAt p m (validFrom m))
                 (sample (verdictAt p m) (laterTicks p m)) eq
... | v , xs , _ , seq , refl =
  compress-inc v xs
    (sample-inc (verdictAt p m) (laterTicks p m)
       (upFrom-inc (suc (validFrom m)) _ (n<1+n (validFrom m))) seq)

-- (c) "Consecutive segments always differ in verdict".
timeline-verdicts-differ :
  ∀ p m {segs} → timeline p m ≡ ok segs →
  ConsecutiveDiffer (map verdict segs)
timeline-verdicts-differ p m eq
  with build-inv (validFrom m) (verdictAt p m (validFrom m))
                 (sample (verdictAt p m) (laterTicks p m)) eq
... | v , xs , _ , _ , refl = compress-alt v xs

-- (d) "A model with status unknown has one segment, no-claim" -- and it is
--     never refused, whatever the package holds.
private
  sample-const : ∀ (f : ℕ → Maybe Verdict) v ts → (∀ t → f t ≡ just v) →
                 Σ[ xs ∈ List (ℕ × Verdict) ]
                   (sample f ts ≡ sampled xs × compress v xs ≡ [])
  sample-const f v []       k = [] , refl , refl
  sample-const f v (t ∷ ts) k with f t | k t | sample f ts | sample-const f v ts k
  ... | .(just v) | refl | .(sampled xs) | xs , refl , c =
    (t , v) ∷ xs , refl , lem v xs c
    where
    ==ᵛ-refl : ∀ w → (w ==ᵛ w) ≡ true
    ==ᵛ-refl agrees   = refl
    ==ᵛ-refl diverges = refl
    ==ᵛ-refl noClaim  = refl

    lem : ∀ w ys → compress w ys ≡ [] → compress w ((t , w) ∷ ys) ≡ []
    lem w ys c′ rewrite ==ᵛ-refl w = c′

unknown-model-one-segment :
  ∀ p m → status m ≡ unknown →
  timeline p m ≡ ok (seg (validFrom m) noClaim ∷ [])
unknown-model-one-segment p m st = go
  where
  always : ∀ t → verdictAt p m t ≡ just noClaim
  always t rewrite st = refl

  go : timeline p m ≡ ok (seg (validFrom m) noClaim ∷ [])
  go with verdictAt p m (validFrom m) | always (validFrom m)
        | sample (verdictAt p m) (laterTicks p m)
        | sample-const (verdictAt p m) noClaim (laterTicks p m) always
  ... | .(just noClaim) | refl | .(sampled xs) | xs , refl , c =
    cong (λ r → ok (seg (validFrom m) noClaim ∷ r)) c

-- (e) "A model whose successor starts on its own valid_from has one
--     segment, at its valid_from."  (More generally: whenever `until` is
--     no later than valid_from + 1, nothing after valid_from is compared.)
successor-on-validFrom-one-segment :
  ∀ p m {segs} → until p m ≡ just (validFrom m) → timeline p m ≡ ok segs →
  Σ[ v ∈ Verdict ] segs ≡ seg (validFrom m) v ∷ []
successor-on-validFrom-one-segment p m {segs} u eq = go eq
  where
  noTicks : laterTicks p m ≡ []
  noTicks rewrite u | m≤n⇒m∸n≡0 (n≤1+n (validFrom m)) = refl

  go : timeline p m ≡ ok segs → Σ[ v ∈ Verdict ] segs ≡ seg (validFrom m) v ∷ []
  go eq′ rewrite noTicks with verdictAt p m (validFrom m)
  go refl | just v = v , refl

-- (f) A timeline of one segment is never stale-divergent, and a
--     stale-divergent timeline starts with `agrees`.
one-segment-not-stale : ∀ s → staleDivergent (s ∷ []) ≡ false
one-segment-not-stale (seg _ agrees)   = refl
one-segment-not-stale (seg _ diverges) = refl
one-segment-not-stale (seg _ noClaim)  = refl

stale-starts-agreeing :
  ∀ s ss → staleDivergent (s ∷ ss) ≡ true → verdict s ≡ agrees
stale-starts-agreeing (seg _ agrees)   ss _ = refl
stale-starts-agreeing (seg _ diverges) ss ()
stale-starts-agreeing (seg _ noClaim)  ss ()

-- (g) `until` really is where the belief stops holding: from `until` on,
--     the belief does not hold (the lifetime's end tick is exclusive).
private
  ∧-false-r : ∀ b → (b ∧ false) ≡ false
  ∧-false-r true  = refl
  ∧-false-r false = refl

  ≤ᵇ-true : ∀ {a b} → a ≤ b → (a ≤ᵇ b) ≡ true
  ≤ᵇ-true {zero}  {b}     _         = refl
  ≤ᵇ-true {suc a} {suc b} (s≤s a≤b) = ≤ᵇ-true {a} {b} a≤b

  ∨-any : ∀ {A : Set} (f : A → Bool) x xs → any f xs ≡ true → any f (x ∷ xs) ≡ true
  ∨-any f x xs eq with f x
  ... | true  = refl
  ... | false = eq

  -- Something in the list starts no later than the least start.
  leastFrom-hit : ∀ (cs : List Belief) x t →
    leastFrom x (map validFrom cs) ≤ t →
    (x ≤ᵇ t) ≡ false → any (startedBy t) cs ≡ true
  leastFrom-hit []       x t le nf with trans (sym (≤ᵇ-true le)) nf
  ... | ()
  leastFrom-hit (c ∷ cs) x t le nf with validFrom c ≤ᵇ t in eqc
  ... | true  = refl
  ... | false = leastFrom-hit cs (x ⊓ validFrom c) t le (min-late x (validFrom c) nf eqc)
    where
    ≤ᵇ-false : ∀ a b → (a ≤ᵇ b) ≡ false → b < a
    ≤ᵇ-false zero    b       ()
    ≤ᵇ-false (suc a) zero    _ = s≤s z≤n
    ≤ᵇ-false (suc a) (suc b) e = s≤s (≤ᵇ-false a b e)

    <⇒≤ᵇ-false : ∀ a b → b < a → (a ≤ᵇ b) ≡ false
    <⇒≤ᵇ-false (suc a) zero    _         = refl
    <⇒≤ᵇ-false (suc a) (suc b) (s≤s lt) = <⇒≤ᵇ-false a b lt

    ⊓-glb : ∀ a b c → c < a → c < b → c < a ⊓ b
    ⊓-glb (suc a) (suc b) zero    _         _         = s≤s z≤n
    ⊓-glb (suc a) (suc b) (suc c) (s≤s p) (s≤s q) = s≤s (⊓-glb a b c p q)

    min-late : ∀ a b → (a ≤ᵇ t) ≡ false → (b ≤ᵇ t) ≡ false → (a ⊓ b ≤ᵇ t) ≡ false
    min-late a b ea eb = <⇒≤ᵇ-false (a ⊓ b) t (⊓-glb a b t (≤ᵇ-false a t ea) (≤ᵇ-false b t eb))

  least-hit : ∀ (cs : List Belief) u t →
    least (map validFrom cs) ≡ just u → u ≤ t → any (startedBy t) cs ≡ true
  least-hit (c ∷ cs) u t refl le with validFrom c ≤ᵇ t in eqc
  ... | true  = refl
  ... | false = leastFrom-hit cs (validFrom c) t le eqc

not-holding-from-until :
  ∀ p b u t → until p b ≡ just u → u ≤ t → holdsAt p b t ≡ false
not-holding-from-until p b u t eq le
  rewrite least-hit (superseders p b) u t eq le = ∧-false-r (startedBy t b)

-- (h) A belief never holds before its valid_from.
private
  late : ∀ a c → c < a → (a ≤ᵇ c) ≡ false
  late (suc a) zero    _         = refl
  late (suc a) (suc c) (s≤s lt′) = late a c lt′

not-holding-before-validFrom :
  ∀ p b t → t < validFrom b → holdsAt p b t ≡ false
not-holding-before-validFrom p b t lt rewrite late (validFrom b) t lt = refl

-- (i) "A model that nothing supersedes holds to the end of the package, and
--     its lifetime has no end tick."  The sweep stops at `horizon`, the last
--     valid_from in the package.  That loses nothing: from the horizon on,
--     no belief starts or stops holding, so the verdict never changes again.
private
  Bounded : ℕ → List Belief → Set
  Bounded h []       = ⊤
  Bounded h (c ∷ cs) = validFrom c ≤ h × Bounded h cs

  Bounded-weaken : ∀ {h h′} cs → h ≤ h′ → Bounded h cs → Bounded h′ cs
  Bounded-weaken []       le _          = tt
  Bounded-weaken (c ∷ cs) le (b , rest) = ≤-trans b le , Bounded-weaken cs le rest

  horizon-bounds : ∀ p → Bounded (horizon p) p
  horizon-bounds []      = tt
  horizon-bounds (b ∷ p) =
    m≤m⊔n (validFrom b) (horizon p) ,
    Bounded-weaken p (m≤n⊔m (validFrom b) (horizon p)) (horizon-bounds p)

  Bounded-filter : ∀ {h} f cs → Bounded h cs → Bounded h (filterᵇ f cs)
  Bounded-filter f []       _          = tt
  Bounded-filter f (c ∷ cs) (b , rest) with f c
  ... | true  = b , Bounded-filter f cs rest
  ... | false = Bounded-filter f cs rest

  started-late : ∀ {h t} c → validFrom c ≤ h → h ≤ t → startedBy t c ≡ startedBy h c
  started-late c b le = trans (≤ᵇ-true (≤-trans b le)) (sym (≤ᵇ-true b))

  any-late : ∀ {h t} cs → Bounded h cs → h ≤ t →
             any (startedBy t) cs ≡ any (startedBy h) cs
  any-late []       _          le = refl
  any-late (c ∷ cs) (b , rest) le = cong₂ _∨_ (started-late c b le) (any-late cs rest le)

  holdsAt-late : ∀ {h t} p c → Bounded h p → validFrom c ≤ h → h ≤ t →
                 holdsAt p c t ≡ holdsAt p c h
  holdsAt-late p c bp b le =
    cong₂ (λ x y → x ∧ not y)
          (started-late c b le)
          (any-late (superseders p c) (Bounded-filter _ p bp) le)

  filter-cong : ∀ {h} (f g : Belief → Bool) cs → Bounded h cs →
                (∀ c → validFrom c ≤ h → f c ≡ g c) → filterᵇ f cs ≡ filterᵇ g cs
  filter-cong f g []       _          k = refl
  filter-cong f g (c ∷ cs) (b , rest) k
    rewrite k c b | filter-cong f g cs rest k = refl

  verdictAt-late : ∀ {h t} p m → Bounded h p → h ≤ t →
                   verdictAt p m t ≡ verdictAt p m h
  verdictAt-late {h} {t} p m bp le =
    cong (λ cs → judge (claim (status m)) (mindHolds cs))
         (filter-cong (λ c → holdsAt p c t) (λ c → holdsAt p c h)
                      (filterᵇ (ofComparedKind m) p)
                      (Bounded-filter (ofComparedKind m) p bp)
                      (λ c b → holdsAt-late p c bp b le))

verdict-settles-at-horizon :
  ∀ p m t → horizon p ≤ t → verdictAt p m t ≡ verdictAt p m (horizon p)
verdict-settles-at-horizon p m t le = verdictAt-late p m (horizon-bounds p) le
