-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
--
-- One key, one holder at a time (ADR-0029, CAC-KERNEL §3 invariant 4).
--
-- The Agda twin of verification/proofs/coq/KeyChain.v: the same model and
-- the same statement, proved independently. A key's beliefs, oldest first,
-- form one chain (rule belief-key-chained); `vf i` is the valid_from of the
-- i-th of `n`. The validator checks each edge (rule supersession-monotone).
-- The reader says the i-th holds at t when vf i ≤ t and its successor, if
-- any, starts after t. Under the edge check, at most one holds at any t.
--
-- Scope: the chain is taken as given; that the validator's checks yield one
-- chain is enforced by code and tests in enaction-contracts, not proved here.

module KeyChain where

open import Data.Nat using (ℕ; zero; suc; _+_; _∸_; _≤_; _<_; z≤n; s≤s)
open import Data.Nat.Properties
  using (≤-refl; ≤-trans; +-identityʳ; +-suc; <-cmp; <⇒≤; m+[n∸m]≡n; <-≤-trans; n≮n; ≤-<-trans)
open import Data.Product using (_×_; _,_)
open import Data.Empty using (⊥-elim)
open import Relation.Binary using (tri<; tri≈; tri>)
open import Relation.Binary.PropositionalEquality using (_≡_; refl; subst; sym)

module Chain (n : ℕ) (vf : ℕ → ℕ) where

  -- What the validator checks, edge by edge.
  AdjacentMonotone : Set
  AdjacentMonotone = ∀ i → suc i < n → vf i ≤ vf (suc i)

  -- The reader's "holds at tick t", for the i-th belief of the chain.
  Holds : ℕ → ℕ → Set
  Holds i t = (i < n) × (vf i ≤ t) × (suc i < n → t < vf (suc i))

  module _ (mono : AdjacentMonotone) where

    -- The local check gives the global order along the chain.
    chain-monotone : ∀ i k → i + k < n → vf i ≤ vf (i + k)
    chain-monotone i zero    _  rewrite +-identityʳ i = ≤-refl
    chain-monotone i (suc k) lt rewrite +-suc i k =
      ≤-trans (chain-monotone i k (<-trans′ lt)) (mono (i + k) lt)
      where
        <-trans′ : suc (i + k) < n → i + k < n
        <-trans′ p = ≤-trans (s≤s (≤-step′ (i + k))) p
          where
            ≤-step′ : ∀ m → m ≤ suc m
            ≤-step′ zero    = z≤n
            ≤-step′ (suc m) = s≤s (≤-step′ m)

    chain-monotone-≤ : ∀ i j → i ≤ j → j < n → vf i ≤ vf j
    chain-monotone-≤ i j i≤j j<n =
      subst (λ x → vf i ≤ vf x) (m+[n∸m]≡n i≤j)
        (chain-monotone i (j ∸ i) (subst (_< n) (sym (m+[n∸m]≡n i≤j)) j<n))

    -- The theorem: at most one belief of a key holds at any tick.
    at-most-one-holds : ∀ i j t → Holds i t → Holds j t → i ≡ j
    at-most-one-holds i j t (i<n , vi≤t , ni) (j<n , vj≤t , nj) with <-cmp i j
    ... | tri≈ _ i≡j _ = i≡j
    ... | tri< i<j _ _ =
      ⊥-elim (n≮n t (<-≤-trans (ni (≤-<-trans i<j j<n))
                              (≤-trans (chain-monotone-≤ (suc i) j i<j j<n) vj≤t)))
    ... | tri> _ _ j<i =
      ⊥-elim (n≮n t (<-≤-trans (nj (≤-<-trans j<i i<n))
                              (≤-trans (chain-monotone-≤ (suc j) i j<i i<n) vi≤t)))
