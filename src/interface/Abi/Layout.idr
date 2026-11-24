-- SPDX-License-Identifier: MPL-2.0
-- Copyright (c) Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
||| ABI Layout Verification
|||
||| This module provides formal proofs about memory layout, alignment,
||| and padding for C-compatible structs.

module Abi.Layout

import Abi.Types
import Data.Vect
import Data.So

%default total

--------------------------------------------------------------------------------
-- Alignment Invariants
--------------------------------------------------------------------------------

||| Predicate: n divides m
public export
data Divides : (n, m : Nat) -> Type where
  MkDivides : (k : Nat) -> (0 prf : m = k * n) -> Divides n m

||| Implementation of divides for common sizes
public export
div8_24 : Divides 8 24
div8_24 = MkDivides 3 Refl

public export
div4_0 : Divides 4 0
div4_0 = MkDivides 0 Refl

public export
div8_8 : Divides 8 8
div8_8 = MkDivides 1 Refl

public export
div8_16 : Divides 8 16
div8_16 = MkDivides 2 Refl

||| Calculate padding required for an offset to meet alignment
public export
paddingFor : (offset : Nat) -> (alignment : Nat) -> Nat
paddingFor offset 0 = 0
paddingFor offset alignment =
  let m = offset `mod` alignment in
  if m == 0
    then 0
    else alignment `minus` m

||| Align a size up to the next multiple of alignment
public export
alignUp : (size : Nat) -> (alignment : Nat) -> Nat
alignUp size alignment =
  size + paddingFor size alignment

--------------------------------------------------------------------------------
-- Struct Model
--------------------------------------------------------------------------------

||| Representation of a single field in a struct
public export
record Field where
  constructor MkField
  name : String
  offset : Nat
  size : Nat
  alignment : Nat

||| Valid memory layout for a C struct
public export
record StructLayout where
  constructor MkStructLayout
  {n : Nat}
  fields : Vect n Field
  totalSize : Nat
  alignment : Nat
  {auto 0 aligned : Divides alignment totalSize}

--------------------------------------------------------------------------------
-- Compliance Predicates
--------------------------------------------------------------------------------

||| Proof that all fields in a struct are correctly aligned
public export
data FieldsAligned : {n : Nat} -> Vect n Field -> Type where
  NoFields : FieldsAligned []
  ConsField :
    {n : Nat} ->
    (f : Field) ->
    (rest : Vect n Field) ->
    (0 prf : Divides f.alignment f.offset) ->
    FieldsAligned rest ->
    FieldsAligned (f :: rest)

||| Proof that every field ends at or before the declared struct size.
public export
data FieldsInBounds : (structSize : Nat) -> {n : Nat} -> Vect n Field -> Type where
  NoBounds : {structSize : Nat} -> FieldsInBounds structSize []
  ConsBound :
    {structSize : Nat} ->
    {n : Nat} ->
    (f : Field) ->
    (rest : Vect n Field) ->
    (0 prf : LTE (f.offset + f.size) structSize) ->
    FieldsInBounds structSize rest ->
    FieldsInBounds structSize (f :: rest)

||| Predicate: Struct is C-ABI compliant
public export
data CABICompliant : StructLayout -> Type where
  CABIOk : (l : StructLayout) ->
           (0 aligned : FieldsAligned l.fields) ->
           (0 bounded : FieldsInBounds l.totalSize l.fields) ->
           CABICompliant l

private
lteReflInternal : {n : Nat} -> LTE n n
lteReflInternal {n = Z} = LTEZero
lteReflInternal {n = S k} = LTESucc lteReflInternal

||| Constructively extend a reflexive bound by a known number of bytes.
public export
ltePlusRight : (left, extra : Nat) -> LTE left (left + extra)
ltePlusRight left Z = rewrite plusZeroRightNeutral left in lteReflInternal
ltePlusRight left (S extra) =
  replace {p = \right => LTE left right}
          (plusSuccRightSucc left extra)
          (lteSuccRight (ltePlusRight left extra))

--------------------------------------------------------------------------------
-- Example and Proofs
--------------------------------------------------------------------------------

||| Example: struct { int32_t x; int64_t y; double z; }
||| On 64-bit Linux, this should have size 24, alignment 8.
public export
exampleLayout : StructLayout
exampleLayout =
  MkStructLayout
    [ MkField "x" 0 4 4     -- Bits32 at offset 0
    , MkField "y" 8 8 8     -- Bits64 at offset 8 (4 bytes padding)
    , MkField "z" 16 8 8    -- Double at offset 16
    ]
    24  -- Total size: 24 bytes
    8   -- Alignment: 8 bytes
    {aligned = div8_24}

||| Proof that example layout is valid
public export
exampleLayoutValid : CABICompliant Abi.Layout.exampleLayout
exampleLayoutValid = CABIOk Abi.Layout.exampleLayout (
  ConsField (MkField "x" 0 4 4) _ div4_0 (
  ConsField (MkField "y" 8 8 8) _ div8_8 (
  ConsField (MkField "z" 16 8 8) _ div8_16 (
  NoFields)))) (
  ConsBound (MkField "x" 0 4 4) _ (ltePlusRight 4 20) (
  ConsBound (MkField "y" 8 8 8) _ (ltePlusRight 16 8) (
  ConsBound (MkField "z" 16 8 8) _ (ltePlusRight 24 0) (
  NoBounds))))
