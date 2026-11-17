(* SPDX-License-Identifier: AGPL-3.0-or-later *)
(* Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk> *)

(** * One key, one holder at a time (ADR-0029, CAC-KERNEL §3 invariant 4)

    The package validator refuses a package unless, for every key
    (holders, bearer, proposition):

    - [belief-key-chained]: the beliefs of the key form ONE chain under
      [supersedes], so they can be listed oldest first as b_0, ..., b_(n-1)
      with b_(i+1) superseding b_i; and
    - [supersession-monotone]: a belief never supersedes one that starts
      after it, so valid_from(b_i) <= valid_from(b_(i+1)).

    The ToM reader (CAC-KERNEL §9) says that b_i HOLDS at tick t when
    valid_from(b_i) <= t and no belief that supersedes b_i has
    valid_from <= t. Inside a chain, the only in-key superseder of b_i is
    b_(i+1). A superseder from another key can only stop b_i from holding,
    which never adds a holder; [at_most_one_holds_guarded] covers it.

    This file proves that under those two package rules at most one belief
    of a key holds at any tick. So on a VALID package the reader's refusal
    [model-counterpart-unique] cannot fire; it stays as defence in depth.

    Scope, stated honestly: the model takes the chain as given. That the
    validator's checks (in-key edge count n - 1, no fork, no cycle) yield
    one chain is enforced by code and tests in [enaction-contracts], not
    proved here. The same statement is proved independently in
    [verification/proofs/agda/KeyChain.agda]. *)

From Coq Require Import Arith Lia.

Section Chain.

  (** A key's chain: [n] beliefs, oldest first; [vf i] is the valid_from
      of the i-th. Indices at or beyond [n] are not beliefs. *)
  Variable n : nat.
  Variable vf : nat -> nat.

  (** What the validator checks, edge by edge. *)
  Definition adjacent_monotone : Prop :=
    forall i, S i < n -> vf i <= vf (S i).

  (** The reader's "holds at tick t", for the i-th belief of the chain. *)
  Definition holds (i t : nat) : Prop :=
    i < n /\ vf i <= t /\ (S i < n -> t < vf (S i)).

  Hypothesis Hmono : adjacent_monotone.

  (** The local check gives the global order along the chain. *)
  Lemma chain_monotone : forall i k, i + k < n -> vf i <= vf (i + k).
  Proof.
    intros i k. induction k as [| k IH]; intro Hlt.
    - rewrite Nat.add_0_r. apply le_n.
    - assert (Hstep : vf (i + k) <= vf (S (i + k))).
      { apply Hmono. lia. }
      replace (i + S k) with (S (i + k)) by lia.
      eapply Nat.le_trans; [ apply IH; lia | exact Hstep ].
  Qed.

  Lemma chain_monotone_le : forall i j, i <= j -> j < n -> vf i <= vf j.
  Proof.
    intros i j Hij Hj.
    replace j with (i + (j - i)) by lia.
    apply chain_monotone. lia.
  Qed.

  (** The theorem: at most one belief of a key holds at any tick. *)
  Theorem at_most_one_holds :
    forall i j t, holds i t -> holds j t -> i = j.
  Proof.
    intros i j t [Hi [Hvi Hni]] [Hj [Hvj Hnj]].
    destruct (Nat.lt_trichotomy i j) as [Hlt | [Heq | Hgt]].
    - exfalso.
      assert (Hnext : t < vf (S i)) by (apply Hni; lia).
      assert (Hle : vf (S i) <= vf j) by (apply chain_monotone_le; lia).
      lia.
    - exact Heq.
    - exfalso.
      assert (Hnext : t < vf (S j)) by (apply Hnj; lia).
      assert (Hle : vf (S j) <= vf i) by (apply chain_monotone_le; lia).
      lia.
  Qed.

  (** A superseder from another key only removes holders: any extra guard
      [g] keeps uniqueness. *)
  Corollary at_most_one_holds_guarded :
    forall (g : nat -> nat -> Prop) i j t,
      holds i t /\ g i t -> holds j t /\ g j t -> i = j.
  Proof.
    intros g i j t [Hi _] [Hj _]. exact (at_most_one_holds i j t Hi Hj).
  Qed.

End Chain.

(** Without [supersession-monotone] the theorem is false. The pre-ADR-0029
    test [a_superseded_model_ends_where_its_successor_begins] allowed a
    successor to start before its predecessor. Witness, a chain of three
    with valid_from 1, 9, 3 (b_2 starts before b_1): at tick 3, b_0 holds
    (1 <= 3 < 9) and b_2 holds (3 <= 3, and it is the last). *)
Example monotone_is_needed :
  let vf := fun i => match i with 0 => 1 | 1 => 9 | _ => 3 end in
  holds 3 vf 0 3 /\ holds 3 vf 2 3 /\ 0 <> 2.
Proof.
  simpl. unfold holds. repeat split; intros; lia.
Qed.
