(* SPDX-License-Identifier: MPL-2.0 *)
(* Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk> *)
(* Independent mechanisation under ADR-0025: written on 2026-11-17 by a
   writer that was given only the prose contract and the seven accepted
   input packages of conformance/tom/v1, and was refused the Rust reader,
   every expected.json and the Agda mechanisation. Its CHOICE-n marks are
   the places where the prose did not decide; ADR-0025 lists them. *)
(* TomSpec.v

   A mechanised reading of CAC-KERNEL section 9, "Models are compared with
   minds" (ADR-0028, ADR-0029), written from the prose alone.

   Standard library only.  No axioms, no Admitted.

   Every place where the prose forced a choice is marked  CHOICE-n  and is
   listed in the report. *)

From Coq Require Import List Arith Bool String Lia Sorted.
Import ListNotations.
Local Open Scope nat_scope.

(* ------------------------------------------------------------------ *)
(* 1. The data model (section 3), cut down to what section 9 reads.    *)
(* ------------------------------------------------------------------ *)

Inductive status := Certain | Believed | Suspected | Rejected | Unknown.

Record belief := mkBelief {
  belief_id   : nat;
  holders     : list string;   (* ascription path, outermost first *)
  bearer      : string;
  proposition : string;
  bstatus     : status;
  valid_from  : nat;
  supersedes  : option nat
}.

Definition package := list belief.

(* "The order of a belief is the length of holders." *)
Definition order (b : belief) : nat := List.length (holders b).

(* "A model is a belief of order 1 or more." *)
Definition is_model (b : belief) : bool :=
  match holders b with [] => false | _ :: _ => true end.

(* "Holds.  A belief holds its proposition when its status is certain,
   believed or suspected." *)
Definition status_holds (s : status) : bool :=
  match s with
  | Certain | Believed | Suspected => true
  | Rejected | Unknown => false
  end.

(* ------------------------------------------------------------------ *)
(* 2. Holding at a tick, and `until`.                                  *)
(* ------------------------------------------------------------------ *)

(* c names b in `supersedes`.  CHOICE-11: the prose does not say that
   belief ids are unique; every belief carrying the named id counts. *)
Definition names (c b : belief) : bool :=
  match supersedes c with
  | Some i => i =? belief_id b
  | None => false
  end.

(* Every belief that supersedes b, "whatever its own bearer, path or
   proposition". *)
Definition superseders (pkg : package) (b : belief) : list belief :=
  filter (fun c => names c b) pkg.

(* "A belief holds at tick t when its valid_from is at most t and no
   belief in the package that names it in supersedes has a valid_from of
   at most t." *)
Definition holds_at (pkg : package) (b : belief) (t : nat) : bool :=
  (valid_from b <=? t)
  && forallb (fun c => negb (valid_from c <=? t)) (superseders pkg b).

Fixpoint min_list (l : list nat) : option nat :=
  match l with
  | [] => None
  | x :: l' =>
      match min_list l' with
      | None => Some x
      | Some y => Some (Nat.min x y)
      end
  end.

(* "until, the earliest valid_from of a belief that supersedes the
   model."  None = nothing supersedes it = no end tick. *)
Definition until (pkg : package) (b : belief) : option nat :=
  min_list (map valid_from (superseders pkg b)).

Definition before_until (u : option nat) (t : nat) : bool :=
  match u with
  | None => true
  | Some e => t <? e
  end.

(* CHOICE-1: the lifetime is the half-open range [valid_from, until).
   The tick `until` itself is NOT a tick of the lifetime. *)
Definition in_lifetime (pkg : package) (m : belief) (t : nat) : bool :=
  (valid_from m <=? t) && before_until (until pkg m) t.

(* ------------------------------------------------------------------ *)
(* 3. The compared belief, the claim, the verdict at one tick.         *)
(* ------------------------------------------------------------------ *)

Fixpoint list_eqb (a b : list string) : bool :=
  match a, b with
  | [], [] => true
  | x :: a', y :: b' => String.eqb x y && list_eqb a' b'
  | _, _ => false
  end.

(* "the belief it is compared with has holders = [h2..hn] (the outermost
   holder removed), bearer = b, the same proposition". *)
Definition compared_kind (m c : belief) : bool :=
  is_model m
  && list_eqb (tl (holders m)) (holders c)
  && String.eqb (bearer m) (bearer c)
  && String.eqb (proposition m) (proposition c).

(* The beliefs of the compared kind that hold at tick t. *)
Definition counterparts_at (pkg : package) (m : belief) (t : nat)
  : list belief :=
  filter (fun c => compared_kind m c && holds_at pkg c t) pkg.

Inductive verdict := Agrees | Diverges | NoClaim.

Definition verdict_eqb (a b : verdict) : bool :=
  match a, b with
  | Agrees, Agrees | Diverges, Diverges | NoClaim, NoClaim => true
  | _, _ => false
  end.

(* "The claim."  Some true: says the modelled belief holds the
   proposition.  Some false: says it does not.  None: no claim. *)
Definition claim (m : belief) : option bool :=
  match bstatus m with
  | Unknown => None
  | s => Some (status_holds s)
  end.

(* "agrees when the compared belief holds the proposition and the model
   says it does, or when neither holds it.  It diverges otherwise." *)
Definition judge (said held : bool) : verdict :=
  if Bool.eqb said held then Agrees else Diverges.

(* The verdict at one tick.  None = undecidable at this tick
   (`model-counterpart-unique`).
   CHOICE-3: at a tick the modelled bearer "holds the proposition" when
   the one compared-kind belief that holds AT THAT TICK has a holding
   STATUS.  No compared-kind belief holding at the tick = does not hold.
   CHOICE-6: an `unknown` model "is compared with nothing", so it is
   never undecidable.
   CHOICE-7: "none supersedes the others by then" is read as direct
   supersession, which holds_at has already applied; two or more
   survivors are therefore always undecidable. *)
Definition verdict_at (pkg : package) (m : belief) (t : nat)
  : option verdict :=
  match claim m with
  | None => Some NoClaim
  | Some said =>
      match counterparts_at pkg m t with
      | [] => Some (judge said false)
      | [c] => Some (judge said (status_holds (bstatus c)))
      | _ :: _ :: _ => None
      end
  end.

(* ------------------------------------------------------------------ *)
(* 4. The timeline.                                                    *)
(* ------------------------------------------------------------------ *)

Definition segment := (nat * verdict)%type.   (* (from_tick, verdict) *)

Inductive outcome :=
| Timeline (segs : list segment)
| Refused (first_tick : nat).   (* model-counterpart-unique *)

(* Sorted, duplicate-free insertion. *)
Fixpoint insert_u (x : nat) (l : list nat) : list nat :=
  match l with
  | [] => [x]
  | y :: l' =>
      if x <? y then x :: y :: l'
      else if x =? y then y :: l'
      else y :: insert_u x l'
  end.

Definition sort_u (l : list nat) : list nat := fold_right insert_u [] l.

(* The ticks after the model's valid_from, inside its lifetime, at which
   the verdict could change.  The prose says a change "can only be a tick
   where a belief of the compared kind starts or stops holding"; a belief
   starts holding at its own valid_from and stops at the valid_from of a
   superseder, so every such tick is the valid_from of some belief in the
   package.  All of them are tried (a superset is harmless: see
   timeline_sound and verdict_stable). *)
Definition change_ticks (pkg : package) (m : belief) : list nat :=
  filter (fun t => (valid_from m <? t) && before_until (until pkg m) t)
         (sort_u (map valid_from pkg)).

(* Evaluate the verdict at each tick; stop at the first undecidable. *)
Fixpoint eval_ticks (pkg : package) (m : belief) (ts : list nat)
  : list segment + nat :=
  match ts with
  | [] => inl []
  | t :: ts' =>
      match verdict_at pkg m t with
      | None => inr t
      | Some v =>
          match eval_ticks pkg m ts' with
          | inl l => inl ((t, v) :: l)
          | inr e => inr e
          end
      end
  end.

(* Keep a point only where the verdict changes. *)
Fixpoint compress (prev : verdict) (l : list segment) : list segment :=
  match l with
  | [] => []
  | (t, v) :: l' =>
      if verdict_eqb v prev then compress prev l'
      else (t, v) :: compress v l'
  end.

(* CHOICE-2: the first segment is always evaluated at the model's own
   valid_from, even when the lifetime is empty (successor on the same
   tick).  The prose says such a model "has one segment, at its
   valid_from" but does not say with which verdict. *)
Definition model_timeline (pkg : package) (m : belief) : outcome :=
  match verdict_at pkg m (valid_from m) with
  | None => Refused (valid_from m)
  | Some v0 =>
      match eval_ticks pkg m (change_ticks pkg m) with
      | inr t => Refused t
      | inl pts => Timeline ((valid_from m, v0) :: compress v0 pts)
      end
  end.

(* ------------------------------------------------------------------ *)
(* 5. Stale divergence.                                                *)
(* ------------------------------------------------------------------ *)

Definition is_diverges (v : verdict) : bool :=
  match v with Diverges => true | _ => false end.

(* "its first segment agrees and a later segment diverges". *)
Definition stale_divergent (segs : list segment) : bool :=
  match segs with
  | (_, Agrees) :: rest => existsb (fun s => is_diverges (snd s)) rest
  | _ => false
  end.

(* ------------------------------------------------------------------ *)
(* 6. The reader's output: every model, in belief-id order.            *)
(* ------------------------------------------------------------------ *)

Record model_report := mkReport {
  mr_id       : nat;
  mr_timeline : outcome;
  mr_until    : option nat;
  mr_stale    : bool
}.

Definition report (pkg : package) (m : belief) : model_report :=
  let tl := model_timeline pkg m in
  {| mr_id := belief_id m;
     mr_timeline := tl;
     mr_until := until pkg m;
     mr_stale := match tl with
                 | Timeline segs => stale_divergent segs
                 | Refused _ => false
                 end |}.

Fixpoint insert_by_id (b : belief) (l : list belief) : list belief :=
  match l with
  | [] => [b]
  | c :: l' =>
      if belief_id b <=? belief_id c then b :: c :: l'
      else c :: insert_by_id b l'
  end.

Definition sort_by_id (l : list belief) : list belief :=
  fold_right insert_by_id [] l.

Definition model_timelines (pkg : package) : list model_report :=
  map (report pkg) (sort_by_id (filter is_model pkg)).

(* The value a timeline gives for a tick: the verdict of the last
   segment that starts at or before it. *)
Fixpoint lookup (d : verdict) (l : list segment) (t : nat) : verdict :=
  match l with
  | [] => d
  | (u, v) :: l' => lookup (if u <=? t then v else d) l' t
  end.

Definition timeline_verdict (segs : list segment) (t : nat)
  : option verdict :=
  match segs with
  | [] => None
  | (_, v0) :: rest => Some (lookup v0 rest t)
  end.

(* "Consecutive segments always differ in verdict." *)
Fixpoint alternating (prev : verdict) (l : list segment) : Prop :=
  match l with
  | [] => True
  | (_, v) :: l' => v <> prev /\ alternating v l'
  end.

(* ================================================================== *)
(* Lemmas                                                              *)
(* ================================================================== *)

Lemma verdict_eqb_spec a b : reflect (a = b) (verdict_eqb a b).
Proof. destruct a, b; constructor; congruence. Qed.

Lemma forallb_ext_in' (A : Type) (f g : A -> bool) (l : list A) :
  (forall x, In x l -> f x = g x) -> forallb f l = forallb g l.
Proof.
  induction l as [|a l IH]; intros H; cbn [forallb]; [reflexivity|].
  rewrite (H a) by (left; reflexivity).
  rewrite IH; [reflexivity|].
  intros x Hx. apply H. right. exact Hx.
Qed.

Lemma filter_none (A : Type) (f : A -> bool) (l : list A) :
  (forall x, f x = false) -> filter f l = [].
Proof.
  intros H. induction l as [|a l IH]; cbn [filter]; [reflexivity|].
  rewrite (H a). exact IH.
Qed.

(* ---- holding at a tick is exactly the half-open lifetime ---------- *)

Lemma superseded_before (l : list belief) (t : nat) :
  forallb (fun c => negb (valid_from c <=? t)) l
  = before_until (min_list (map valid_from l)) t.
Proof.
  induction l as [|x l IH]; cbn [forallb map min_list]; [reflexivity|].
  rewrite IH.
  destruct (min_list (map valid_from l)) as [y|]; cbn [before_until].
  - destruct (Nat.leb_spec (valid_from x) t), (Nat.ltb_spec t y),
      (Nat.ltb_spec t (Nat.min (valid_from x) y));
      cbn [negb andb]; try reflexivity; lia.
  - destruct (Nat.leb_spec (valid_from x) t),
      (Nat.ltb_spec t (valid_from x));
      cbn [negb andb]; try reflexivity; lia.
Qed.

(* A belief holds at t exactly when valid_from <= t < until. *)
Theorem holds_at_lifetime (pkg : package) (b : belief) (t : nat) :
  holds_at pkg b t = in_lifetime pkg b t.
Proof.
  unfold holds_at, in_lifetime, until.
  rewrite superseded_before. reflexivity.
Qed.

(* "A successor never starts earlier": with that premise, a belief does
   not hold at its `until` tick, and a model whose successor starts on its
   own valid_from never holds at all. *)
Corollary not_holding_at_until (pkg : package) (b : belief) (u : nat) :
  until pkg b = Some u -> holds_at pkg b u = false.
Proof.
  intros H. rewrite holds_at_lifetime. unfold in_lifetime. rewrite H.
  cbn [before_until]. rewrite Nat.ltb_irrefl. apply andb_false_r.
Qed.

Lemma before_until_mono (u : option nat) (a b : nat) :
  a <= b -> before_until u b = true -> before_until u a = true.
Proof.
  destruct u as [e|]; cbn [before_until]; [|auto].
  rewrite !Nat.ltb_lt. lia.
Qed.

(* ---- sorting ------------------------------------------------------- *)

Lemma insert_u_In (x : nat) (l : list nat) (y : nat) :
  In y (insert_u x l) <-> y = x \/ In y l.
Proof.
  induction l as [|z l IH]; cbn [insert_u].
  - cbn [In]. split; intros [H|[]]; left; symmetry; exact H.
  - destruct (Nat.ltb_spec x z) as [Hlt|Hge].
    + cbn [In]. split; (intros [H|H]; [left; symmetry; exact H|right; exact H]).
    + destruct (Nat.eqb_spec x z) as [E|N].
      * subst. cbn [In]. split.
        -- intros H. right. exact H.
        -- intros [H|H]; [left; symmetry; exact H|exact H].
      * change (In y (z :: insert_u x l)) with (z = y \/ In y (insert_u x l)).
        rewrite IH. cbn [In]. tauto.
Qed.

Lemma insert_u_sorted (x : nat) (l : list nat) :
  StronglySorted lt l -> StronglySorted lt (insert_u x l).
Proof.
  induction l as [|z l IH]; intros Hs; cbn [insert_u].
  - constructor; constructor.
  - inversion Hs as [|? ? Hs' Hall]; subst.
    destruct (Nat.ltb_spec x z) as [Hlt|Hge].
    + constructor; [exact Hs|].
      constructor; [exact Hlt|].
      eapply Forall_impl; [|exact Hall]. intros a Ha. lia.
    + destruct (Nat.eqb_spec x z) as [E|N].
      * exact Hs.
      * constructor; [apply IH; exact Hs'|].
        apply Forall_forall. intros y Hy.
        apply insert_u_In in Hy. destruct Hy as [->|Hy]; [lia|].
        rewrite Forall_forall in Hall. apply Hall. exact Hy.
Qed.

Lemma sort_u_sorted (l : list nat) : StronglySorted lt (sort_u l).
Proof.
  induction l as [|a l IH]; cbn [sort_u fold_right]; [constructor|].
  apply insert_u_sorted. exact IH.
Qed.

Lemma sort_u_In (l : list nat) (y : nat) : In y (sort_u l) <-> In y l.
Proof.
  induction l as [|a l IH]; cbn [sort_u fold_right]; [tauto|].
  rewrite insert_u_In. fold (sort_u l). rewrite IH. cbn [In].
  split; (intros [H|H]; [left; symmetry; exact H|right; exact H]).
Qed.

Lemma filter_sorted (f : nat -> bool) (l : list nat) :
  StronglySorted lt l -> StronglySorted lt (filter f l).
Proof.
  induction l as [|a l IH]; intros Hs; cbn [filter]; [constructor|].
  inversion Hs as [|? ? Hs' Hall]; subst.
  destruct (f a); [|apply IH; exact Hs'].
  constructor; [apply IH; exact Hs'|].
  apply Forall_forall. intros y Hy. apply filter_In in Hy.
  destruct Hy as [Hy _]. rewrite Forall_forall in Hall. apply Hall. exact Hy.
Qed.

Lemma change_ticks_In (pkg : package) (m : belief) (e : nat) :
  In e (change_ticks pkg m)
  <-> In e (map valid_from pkg)
      /\ valid_from m < e
      /\ before_until (until pkg m) e = true.
Proof.
  unfold change_ticks.
  rewrite filter_In, sort_u_In, andb_true_iff, Nat.ltb_lt. tauto.
Qed.

Lemma change_ticks_sorted (pkg : package) (m : belief) :
  StronglySorted lt (valid_from m :: change_ticks pkg m).
Proof.
  constructor.
  - unfold change_ticks. apply filter_sorted, sort_u_sorted.
  - apply Forall_forall. intros e He. apply change_ticks_In in He.
    destruct He as (_ & H & _). exact H.
Qed.

(* ---- evaluation and compression ----------------------------------- *)

Lemma eval_ticks_inl (pkg : package) (m : belief) :
  forall ts pts,
    eval_ticks pkg m ts = inl pts ->
    map fst pts = ts
    /\ Forall (fun p => verdict_at pkg m (fst p) = Some (snd p)) pts.
Proof.
  induction ts as [|t ts IH]; cbn [eval_ticks]; intros pts H.
  - inversion H; subst. split; [reflexivity|constructor].
  - destruct (verdict_at pkg m t) as [v|] eqn:Hv; [|discriminate].
    destruct (eval_ticks pkg m ts) as [l|e] eqn:He; [|discriminate].
    inversion H; subst.
    destruct (IH l eq_refl) as [Hf Ha].
    split; [cbn [map fst]; f_equal; exact Hf|].
    constructor; [exact Hv|exact Ha].
Qed.

Lemma compress_Forall (Q : segment -> Prop) :
  forall l prev, Forall Q l -> Forall Q (compress prev l).
Proof.
  induction l as [|[t v] l IH]; intros prev H; cbn [compress];
    [constructor|].
  inversion H; subst.
  destruct (verdict_eqb v prev).
  - apply IH. assumption.
  - constructor; [assumption|apply IH; assumption].
Qed.

Lemma fst_Forall (P : nat -> Prop) (l : list segment) :
  Forall P (map fst l) <-> Forall (fun p => P (fst p)) l.
Proof.
  rewrite !Forall_forall. split.
  - intros H p Hp. apply H. apply in_map. exact Hp.
  - intros H x Hx. apply in_map_iff in Hx.
    destruct Hx as (p & <- & Hp). apply H. exact Hp.
Qed.

Lemma compress_sorted :
  forall l prev,
    StronglySorted lt (map fst l) ->
    StronglySorted lt (map fst (compress prev l)).
Proof.
  induction l as [|[t v] l IH]; intros prev H; cbn [compress];
    [constructor|].
  cbn [map fst] in H. inversion H as [|? ? Hs Hall]; subst.
  destruct (verdict_eqb v prev); [apply IH; exact Hs|].
  cbn [map fst]. constructor; [apply IH; exact Hs|].
  apply fst_Forall. apply compress_Forall. apply fst_Forall. exact Hall.
Qed.

Lemma compress_alternating :
  forall l prev, alternating prev (compress prev l).
Proof.
  induction l as [|[t v] l IH]; intros prev; cbn [compress]; [exact I|].
  destruct (verdict_eqb_spec v prev) as [E|N]; [apply IH|].
  cbn [alternating]. split; [exact N|apply IH].
Qed.

Lemma lookup_all_gt :
  forall l d t, Forall (fun p => t < fst p) l -> lookup d l t = d.
Proof.
  induction l as [|[u v] l IH]; intros d t H; cbn [lookup]; [reflexivity|].
  inversion H as [|? ? Hu Hl]; subst. cbn [fst] in Hu.
  destruct (Nat.leb_spec u t); [lia|]. apply IH. exact Hl.
Qed.

Lemma lookup_cases :
  forall l d t, lookup d l t = d \/ exists u, In (u, lookup d l t) l.
Proof.
  induction l as [|[u v] l IH]; intros d t; cbn [lookup];
    [left; reflexivity|].
  destruct (IH (if u <=? t then v else d) t) as [E|[w Hw]].
  - rewrite E. destruct (u <=? t).
    + right. exists u. left. reflexivity.
    + left. reflexivity.
  - right. exists w. right. exact Hw.
Qed.

(* Compression does not change what the timeline says at any tick. *)
Lemma compress_lookup :
  forall l prev t,
    StronglySorted lt (map fst l) ->
    lookup prev (compress prev l) t = lookup prev l t.
Proof.
  induction l as [|[u v] l IH]; intros prev t H; cbn [compress lookup];
    [reflexivity|].
  cbn [map fst] in H. inversion H as [|? ? Hs Hall]; subst.
  destruct (verdict_eqb_spec v prev) as [E|N].
  - subst v. rewrite IH by exact Hs. destruct (u <=? t); reflexivity.
  - cbn [lookup]. destruct (Nat.leb_spec u t) as [Hle|Hgt].
    + apply IH. exact Hs.
    + assert (Hgt' : Forall (fun p => t < fst p) l).
      { apply fst_Forall in Hall. eapply Forall_impl; [|exact Hall].
        intros p Hp. cbn beta in Hp. lia. }
      rewrite (lookup_all_gt (compress v l) prev t)
        by (apply compress_Forall; exact Hgt').
      rewrite (lookup_all_gt l prev t) by exact Hgt'.
      reflexivity.
Qed.

(* ---- the verdict changes only at a belief's valid_from ------------- *)

Lemma holds_at_stable (pkg : package) (a b : nat) :
  (forall x, In x pkg -> (valid_from x <=? a) = (valid_from x <=? b)) ->
  forall c, In c pkg -> holds_at pkg c a = holds_at pkg c b.
Proof.
  intros H c Hc. unfold holds_at. rewrite (H c Hc). f_equal.
  apply forallb_ext_in'. intros x Hx.
  unfold superseders in Hx. apply filter_In in Hx. destruct Hx as [Hx _].
  rewrite (H x Hx). reflexivity.
Qed.

(* The prose: a new segment "can only be a tick where a belief of the
   compared kind starts or stops holding".  Checked form: between two
   ticks with no belief's valid_from in (a, b], the verdict is the same. *)
Theorem verdict_stable (pkg : package) (m : belief) (a b : nat) :
  a <= b ->
  (forall e, In e (map valid_from pkg) -> ~ (a < e <= b)) ->
  verdict_at pkg m a = verdict_at pkg m b.
Proof.
  intros Hab Hno.
  assert (H : forall x, In x pkg ->
                        (valid_from x <=? a) = (valid_from x <=? b)).
  { intros x Hx.
    specialize (Hno (valid_from x) (in_map valid_from pkg x Hx)).
    destruct (Nat.leb_spec (valid_from x) a),
      (Nat.leb_spec (valid_from x) b); try reflexivity; lia. }
  assert (E : counterparts_at pkg m a = counterparts_at pkg m b).
  { unfold counterparts_at. apply filter_ext_in. intros c Hc.
    rewrite (holds_at_stable pkg a b H c Hc). reflexivity. }
  unfold verdict_at. rewrite E. reflexivity.
Qed.

Lemma points_lookup (pkg : package) (m : belief) :
  forall ts pts t0 d t,
    eval_ticks pkg m ts = inl pts ->
    verdict_at pkg m t0 = Some d ->
    StronglySorted lt (t0 :: ts) ->
    t0 <= t ->
    (forall e, In e (map valid_from pkg) -> t0 < e <= t -> In e ts) ->
    verdict_at pkg m t = Some (lookup d pts t).
Proof.
  induction ts as [|u ts IH]; intros pts t0 d t He H0 Hs Hle Hev.
  - cbn [eval_ticks] in He. inversion He; subst. cbn [lookup].
    rewrite <- H0. symmetry. apply verdict_stable; [exact Hle|].
    intros e Hin Hr. exact (Hev e Hin Hr).
  - cbn [eval_ticks] in He.
    destruct (verdict_at pkg m u) as [v|] eqn:Hv; [|discriminate].
    destruct (eval_ticks pkg m ts) as [l|x] eqn:Hl; [|discriminate].
    inversion He; subst. cbn [lookup].
    inversion Hs as [|? ? Hs' Hall]; subst.
    inversion Hall as [|? ? Hu Hall']; subst.
    destruct (Nat.leb_spec u t) as [Hut|Htu].
    + apply (IH l u v t eq_refl Hv Hs' Hut).
      intros e Hin Hr.
      assert (Hr' : t0 < e <= t) by lia.
      destruct (Hev e Hin Hr') as [E|Hin']; [lia|exact Hin'].
    + inversion Hs' as [|? ? _ Hgt]; subst.
      pose proof (eval_ticks_inl pkg m ts l Hl) as [Hf _].
      rewrite lookup_all_gt.
      * rewrite <- H0. symmetry. apply verdict_stable; [exact Hle|].
        intros e Hin Hr.
        destruct (Hev e Hin Hr) as [E|Hin']; [lia|].
        rewrite Forall_forall in Hgt. specialize (Hgt e Hin'). lia.
      * apply fst_Forall. rewrite Hf.
        eapply Forall_impl; [|exact Hgt]. intros a Ha. cbn beta. lia.
Qed.

(* ---- the shape of a timeline --------------------------------------- *)

Lemma model_timeline_inv (pkg : package) (m : belief) (segs : list segment) :
  model_timeline pkg m = Timeline segs ->
  exists v0 pts,
    verdict_at pkg m (valid_from m) = Some v0
    /\ eval_ticks pkg m (change_ticks pkg m) = inl pts
    /\ segs = (valid_from m, v0) :: compress v0 pts.
Proof.
  unfold model_timeline.
  destruct (verdict_at pkg m (valid_from m)) as [v0|] eqn:H0; [|discriminate].
  destruct (eval_ticks pkg m (change_ticks pkg m)) as [pts|x] eqn:He;
    [|discriminate].
  intros H. inversion H; subst.
  exists v0, pts. repeat split; reflexivity.
Qed.

(* LEMMA A.  "The first segment starts at the model's valid_from"; the
   from_ticks are strictly increasing; "consecutive segments always
   differ in verdict". *)
Theorem timeline_wellformed (pkg : package) (m : belief)
        (segs : list segment) :
  model_timeline pkg m = Timeline segs ->
  exists v0 rest,
    segs = (valid_from m, v0) :: rest
    /\ StronglySorted lt (map fst segs)
    /\ alternating v0 rest.
Proof.
  intros H. destruct (model_timeline_inv pkg m segs H) as (v0 & pts & H0 & He & ->).
  exists v0, (compress v0 pts). split; [reflexivity|].
  destruct (eval_ticks_inl pkg m _ pts He) as [Hf _].
  pose proof (change_ticks_sorted pkg m) as Hs.
  rewrite <- Hf in Hs. inversion Hs as [|? ? Hs' Hall]; subst.
  split; [|apply compress_alternating].
  cbn [map fst]. constructor; [apply compress_sorted; exact Hs'|].
  apply fst_Forall. apply compress_Forall. apply fst_Forall. exact Hall.
Qed.

(* LEMMA B.  "A model is compared at every tick of its lifetime": at
   every tick of the lifetime, the verdict at that tick is the verdict of
   the last timeline segment that starts at or before it.  So the
   change-point construction loses nothing against a tick-by-tick scan. *)
Theorem timeline_sound (pkg : package) (m : belief) (segs : list segment) :
  model_timeline pkg m = Timeline segs ->
  forall t, in_lifetime pkg m t = true ->
            verdict_at pkg m t = timeline_verdict segs t.
Proof.
  intros H t Hin.
  destruct (model_timeline_inv pkg m segs H) as (v0 & pts & H0 & He & ->).
  cbn [timeline_verdict].
  unfold in_lifetime in Hin. apply andb_true_iff in Hin.
  destruct Hin as [Hge Hbu]. apply Nat.leb_le in Hge.
  destruct (eval_ticks_inl pkg m _ pts He) as [Hf _].
  rewrite compress_lookup.
  - apply (points_lookup pkg m (change_ticks pkg m) pts (valid_from m) v0 t
             He H0 (change_ticks_sorted pkg m) Hge).
    intros e He1 [He2 He3]. apply change_ticks_In.
    split; [exact He1|]. split; [exact He2|].
    exact (before_until_mono _ e t He3 Hbu).
  - rewrite Hf. pose proof (change_ticks_sorted pkg m) as Hs.
    inversion Hs; assumption.
Qed.

(* "a timeline of one segment means that the verdict never changed". *)
Corollary one_segment_never_changed (pkg : package) (m : belief)
          (t0 : nat) (v : verdict) :
  model_timeline pkg m = Timeline [(t0, v)] ->
  forall t, in_lifetime pkg m t = true -> verdict_at pkg m t = Some v.
Proof.
  intros H t Hin. rewrite (timeline_sound pkg m _ H t Hin). reflexivity.
Qed.

(* LEMMA C.  "A model with status unknown has one segment, no-claim." *)
Lemma eval_const (pkg : package) (m : belief) (v : verdict) :
  (forall t, verdict_at pkg m t = Some v) ->
  forall ts, exists pts,
      eval_ticks pkg m ts = inl pts /\ compress v pts = [].
Proof.
  intros H. induction ts as [|a ts IH]; cbn [eval_ticks].
  - exists []. split; reflexivity.
  - rewrite (H a). destruct IH as (pts & E & C). rewrite E.
    exists ((a, v) :: pts). split; [reflexivity|].
    cbn [compress]. destruct (verdict_eqb_spec v v); [exact C|congruence].
Qed.

Theorem unknown_one_segment (pkg : package) (m : belief) :
  bstatus m = Unknown ->
  model_timeline pkg m = Timeline [(valid_from m, NoClaim)].
Proof.
  intros Hu.
  assert (H : forall t, verdict_at pkg m t = Some NoClaim).
  { intros t. unfold verdict_at, claim. rewrite Hu. reflexivity. }
  unfold model_timeline. rewrite H.
  destruct (eval_const pkg m NoClaim H (change_ticks pkg m)) as (pts & E & C).
  rewrite E, C. reflexivity.
Qed.

(* LEMMA D.  "A model whose successor starts on its own valid_from has
   one segment, at its valid_from."  Its lifetime is empty, and (under
   CHOICE-2) the timeline is NOT empty. *)
Theorem successor_on_same_tick (pkg : package) (m : belief) :
  until pkg m = Some (valid_from m) ->
  (forall t, in_lifetime pkg m t = false)
  /\ model_timeline pkg m
     = match verdict_at pkg m (valid_from m) with
       | Some v => Timeline [(valid_from m, v)]
       | None => Refused (valid_from m)
       end.
Proof.
  intros H. split.
  - intros t. unfold in_lifetime. rewrite H. cbn [before_until].
    destruct (Nat.leb_spec (valid_from m) t),
      (Nat.ltb_spec t (valid_from m)); cbn [andb]; try reflexivity; lia.
  - assert (E : change_ticks pkg m = []).
    { unfold change_ticks. rewrite H. apply filter_none. intros x.
      cbn [before_until].
      destruct (Nat.ltb_spec (valid_from m) x),
        (Nat.ltb_spec x (valid_from m)); cbn [andb]; try reflexivity; lia. }
    unfold model_timeline. rewrite E. cbn [eval_ticks compress].
    destruct (verdict_at pkg m (valid_from m)); reflexivity.
Qed.

(* LEMMA E.  Stale divergence, in terms of ticks: the model agrees at its
   valid_from and diverges at some later tick of its lifetime.  "It was
   right when it was stated and went wrong without being restated." *)
Theorem stale_divergent_iff (pkg : package) (m : belief)
        (segs : list segment) :
  model_timeline pkg m = Timeline segs ->
  (stale_divergent segs = true
   <-> verdict_at pkg m (valid_from m) = Some Agrees
       /\ exists t, valid_from m < t
                    /\ in_lifetime pkg m t = true
                    /\ verdict_at pkg m t = Some Diverges).
Proof.
  intros H.
  pose proof (timeline_sound pkg m segs H) as Hsound.
  destruct (model_timeline_inv pkg m segs H) as (v0 & pts & H0 & He & ->).
  destruct (eval_ticks_inl pkg m _ pts He) as [Hf Hv].
  split.
  - intros Hst. cbn [stale_divergent] in Hst.
    destruct v0; try discriminate.
    split; [exact H0|].
    apply existsb_exists in Hst. destruct Hst as ([t v] & Hin & Hd).
    cbn [snd] in Hd. destruct v; try discriminate.
    assert (HQ : Forall (fun p => verdict_at pkg m (fst p) = Some (snd p)
                                  /\ In (fst p) (change_ticks pkg m))
                        (compress Agrees pts)).
    { apply compress_Forall. rewrite Forall_forall in *.
      intros p Hp. split; [apply Hv; exact Hp|].
      rewrite <- Hf. apply in_map. exact Hp. }
    rewrite Forall_forall in HQ. destruct (HQ _ Hin) as [Hvt Hct].
    cbn [fst snd] in Hvt, Hct. apply change_ticks_In in Hct.
    destruct Hct as (_ & Hlt & Hbu).
    exists t. split; [exact Hlt|]. split; [|exact Hvt].
    unfold in_lifetime. rewrite Hbu.
    destruct (Nat.leb_spec (valid_from m) t); [reflexivity|lia].
  - intros [Ha (t & Hlt & Hin & Hd)].
    rewrite H0 in Ha. inversion Ha; subst v0.
    cbn [stale_divergent].
    rewrite (Hsound t Hin) in Hd. cbn [timeline_verdict] in Hd.
    inversion Hd as [Hd'].
    destruct (lookup_cases (compress Agrees pts) Agrees t) as [E|[u Hu]].
    + rewrite E in Hd'. discriminate.
    + rewrite Hd' in Hu. apply existsb_exists.
      exists (u, Diverges). split; [exact Hu|reflexivity].
Qed.

(* A stale-divergent timeline has at least two segments. *)
Corollary stale_needs_two_segments (segs : list segment) :
  stale_divergent segs = true -> 2 <= List.length segs.
Proof.
  destruct segs as [|[t v] [|s rest]]; cbn [stale_divergent existsb];
    try discriminate.
  - destruct v; discriminate.
  - intros _. cbn [List.length]. lia.
Qed.

(* ------------------------------------------------------------------ *)
(* Assumption audit.                                                   *)
(* ------------------------------------------------------------------ *)

Print Assumptions holds_at_lifetime.
Print Assumptions not_holding_at_until.
Print Assumptions verdict_stable.
Print Assumptions timeline_wellformed.
Print Assumptions timeline_sound.
Print Assumptions one_segment_never_changed.
Print Assumptions unknown_one_segment.
Print Assumptions successor_on_same_tick.
Print Assumptions stale_divergent_iff.
Print Assumptions stale_needs_two_segments.
