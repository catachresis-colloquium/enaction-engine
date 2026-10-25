-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
||| Versioned, operation-first accelerator ABI authority.
|||
||| This module is the source of truth for numeric discriminants and native
||| memory layouts. Generated declarations are rendered from these values by
||| `Abi.Generate`; Rust and Zig must not invent competing representations.

module Abi.Accelerator

import Abi.Layout
import Data.Bits
import Data.Vect

%default total

public export
abiMajor : Bits16
abiMajor = 1

public export
abiMinor : Bits16
abiMinor = 0

public export
operationMajor : Bits16
operationMajor = 1

public export
operationMinor : Bits16
operationMinor = 0

public export
data Operation
  = FixedI32Dot
  | FixedI32MatMul
  | TensorF32Relu
  | TensorF32Relu6
  | TensorF32MatMul
  | TensorF32Add
  | TensorF32Mul

public export
operationCode : Operation -> Bits32
operationCode FixedI32Dot = 1
operationCode FixedI32MatMul = 2
operationCode TensorF32Relu = 3
operationCode TensorF32Relu6 = 4
operationCode TensorF32MatMul = 5
operationCode TensorF32Add = 6
operationCode TensorF32Mul = 7

public export
data LayoutTag = DotLayout | MatMulLayout | VectorLayout

public export
layoutCode : LayoutTag -> Bits32
layoutCode DotLayout = 1
layoutCode MatMulLayout = 2
layoutCode VectorLayout = 3

public export
data Lane = Authoritative | Advisory | RemoteJob

public export
laneCode : Lane -> Bits32
laneCode Authoritative = 1
laneCode Advisory = 2
laneCode RemoteJob = 3

public export
data Determinism = AdvisoryOnly | ToleranceBounded | CanonicalExact

public export
determinismCode : Determinism -> Bits32
determinismCode AdvisoryOnly = 1
determinismCode ToleranceBounded = 2
determinismCode CanonicalExact = 3

public export
data SupportLevel
  = Declared
  | Discoverable
  | Loadable
  | Runnable
  | Conformant
  | Resilient
  | Deterministic
  | Benchmarked
  | ProductionSupported

public export
supportCode : SupportLevel -> Bits32
supportCode Declared = 1
supportCode Discoverable = 2
supportCode Loadable = 3
supportCode Runnable = 4
supportCode Conformant = 5
supportCode Resilient = 6
supportCode Deterministic = 7
supportCode Benchmarked = 8
supportCode ProductionSupported = 9

public export
data DeviceClass = Cpu | Gpu | Tpu | Npu | Dsp | Ppu | Math | Fpga | Vpu | Qpu | Crypto

public export
deviceCode : DeviceClass -> Bits32
deviceCode Cpu = 1
deviceCode Gpu = 2
deviceCode Tpu = 3
deviceCode Npu = 4
deviceCode Dsp = 5
deviceCode Ppu = 6
deviceCode Math = 7
deviceCode Fpga = 8
deviceCode Vpu = 9
deviceCode Qpu = 10
deviceCode Crypto = 11

public export
data Status
  = StatusOk
  | NullPointer
  | UnsupportedAbi
  | UnsupportedOperationVersion
  | UnknownOperation
  | InvalidLane
  | InvalidSupport
  | InvalidDeterminism
  | LayoutMismatch
  | LengthMismatch
  | DimensionOverflow
  | ArithmeticOverflow
  | InvalidReservedField
  | UnsupportedRequirement
  | IndexOutOfRange
  | AliasingViolation
  | NonFiniteInput

public export
statusCode : Status -> Bits32
statusCode StatusOk = 0
statusCode NullPointer = 1
statusCode UnsupportedAbi = 2
statusCode UnsupportedOperationVersion = 3
statusCode UnknownOperation = 4
statusCode InvalidLane = 5
statusCode InvalidSupport = 6
statusCode InvalidDeterminism = 7
statusCode LayoutMismatch = 8
statusCode LengthMismatch = 9
statusCode DimensionOverflow = 10
statusCode ArithmeticOverflow = 11
statusCode InvalidReservedField = 12
statusCode UnsupportedRequirement = 13
statusCode IndexOutOfRange = 14
statusCode AliasingViolation = 15
statusCode NonFiniteInput = 16

||| Abstract presence of a data pointer at the accelerator buffer boundary.
||| The generated ABI carries a nullable pointer plus a length; address values
||| themselves remain runtime data and are deliberately not invented here.
public export
data BufferPointerPresence = NullData | DataPresent

||| Normative minimum validity rule for buffer data pointers. Empty buffers may
||| use a null data pointer; a non-empty buffer must carry a data pointer.
||| Individual operations may impose stricter output-shape requirements.
public export
bufferDataPointerValid : BufferPointerPresence -> Nat -> Bool
bufferDataPointerValid NullData Z = True
bufferDataPointerValid NullData (S _) = False
bufferDataPointerValid DataPresent _ = True

||| A valid non-empty buffer necessarily carries a data pointer.
public export
nonEmptyBufferRequiresData : (pointer : BufferPointerPresence) -> (length : Nat) ->
                             bufferDataPointerValid pointer (S length) = True ->
                             pointer = DataPresent
nonEmptyBufferRequiresData NullData length Refl impossible
nonEmptyBufferRequiresData DataPresent length valid = Refl

||| Conversely, a valid null data pointer necessarily denotes an empty buffer.
public export
nullDataOnlyForEmptyBuffer : (length : Nat) ->
                             bufferDataPointerValid NullData length = True ->
                             length = 0
nullDataOnlyForEmptyBuffer Z valid = Refl
nullDataOnlyForEmptyBuffer (S length) Refl impossible

||| A present data pointer satisfies the minimum pointer rule at every length.
public export
presentDataAlwaysValid : (length : Nat) ->
                         bufferDataPointerValid DataPresent length = True
presentDataAlwaysValid length = Refl

||| A host accepts its ABI major and no operation minor newer than it knows.
public export
acceptsVersion : Bits16 -> Bits16 -> Bits16 -> Bits16 -> Bool
acceptsVersion hostMajor hostMinor requestedMajor requestedMinor =
  hostMajor == requestedMajor && requestedMinor <= hostMinor

public export
acceptsCurrentVersion : acceptsVersion Abi.Accelerator.abiMajor Abi.Accelerator.abiMinor 1 0 = True
acceptsCurrentVersion = Refl

public export
rejectsWrongMajor : acceptsVersion Abi.Accelerator.abiMajor Abi.Accelerator.abiMinor 2 0 = False
rejectsWrongMajor = Refl

public export
rejectsNewerMinor : acceptsVersion Abi.Accelerator.abiMajor Abi.Accelerator.abiMinor 1 1 = False
rejectsNewerMinor = Refl

||| The numeric operation discriminant is injective over every admitted
||| operation. This covers all 49 ordered constructor pairs (the seven equal
||| pairs and all 42 unequal directions), rather than sampling selected pairs.
public export
operationCodeInjective : (left, right : Operation) ->
                         operationCode left = operationCode right ->
                         left = right
operationCodeInjective FixedI32Dot FixedI32Dot Refl = Refl
operationCodeInjective FixedI32MatMul FixedI32MatMul Refl = Refl
operationCodeInjective TensorF32Relu TensorF32Relu Refl = Refl
operationCodeInjective TensorF32Relu6 TensorF32Relu6 Refl = Refl
operationCodeInjective TensorF32MatMul TensorF32MatMul Refl = Refl
operationCodeInjective TensorF32Add TensorF32Add Refl = Refl
operationCodeInjective TensorF32Mul TensorF32Mul Refl = Refl
-- Keep both constructors concrete: a wildcard impossible clause does not
-- expose the unequal Bits32 literals to Idris2's impossibility checker.
operationCodeInjective FixedI32Dot FixedI32MatMul Refl impossible
operationCodeInjective FixedI32Dot TensorF32Relu Refl impossible
operationCodeInjective FixedI32Dot TensorF32Relu6 Refl impossible
operationCodeInjective FixedI32Dot TensorF32MatMul Refl impossible
operationCodeInjective FixedI32Dot TensorF32Add Refl impossible
operationCodeInjective FixedI32Dot TensorF32Mul Refl impossible
operationCodeInjective FixedI32MatMul FixedI32Dot Refl impossible
operationCodeInjective FixedI32MatMul TensorF32Relu Refl impossible
operationCodeInjective FixedI32MatMul TensorF32Relu6 Refl impossible
operationCodeInjective FixedI32MatMul TensorF32MatMul Refl impossible
operationCodeInjective FixedI32MatMul TensorF32Add Refl impossible
operationCodeInjective FixedI32MatMul TensorF32Mul Refl impossible
operationCodeInjective TensorF32Relu FixedI32Dot Refl impossible
operationCodeInjective TensorF32Relu FixedI32MatMul Refl impossible
operationCodeInjective TensorF32Relu TensorF32Relu6 Refl impossible
operationCodeInjective TensorF32Relu TensorF32MatMul Refl impossible
operationCodeInjective TensorF32Relu TensorF32Add Refl impossible
operationCodeInjective TensorF32Relu TensorF32Mul Refl impossible
operationCodeInjective TensorF32Relu6 FixedI32Dot Refl impossible
operationCodeInjective TensorF32Relu6 FixedI32MatMul Refl impossible
operationCodeInjective TensorF32Relu6 TensorF32Relu Refl impossible
operationCodeInjective TensorF32Relu6 TensorF32MatMul Refl impossible
operationCodeInjective TensorF32Relu6 TensorF32Add Refl impossible
operationCodeInjective TensorF32Relu6 TensorF32Mul Refl impossible
operationCodeInjective TensorF32MatMul FixedI32Dot Refl impossible
operationCodeInjective TensorF32MatMul FixedI32MatMul Refl impossible
operationCodeInjective TensorF32MatMul TensorF32Relu Refl impossible
operationCodeInjective TensorF32MatMul TensorF32Relu6 Refl impossible
operationCodeInjective TensorF32MatMul TensorF32Add Refl impossible
operationCodeInjective TensorF32MatMul TensorF32Mul Refl impossible
operationCodeInjective TensorF32Add FixedI32Dot Refl impossible
operationCodeInjective TensorF32Add FixedI32MatMul Refl impossible
operationCodeInjective TensorF32Add TensorF32Relu Refl impossible
operationCodeInjective TensorF32Add TensorF32Relu6 Refl impossible
operationCodeInjective TensorF32Add TensorF32MatMul Refl impossible
operationCodeInjective TensorF32Add TensorF32Mul Refl impossible
operationCodeInjective TensorF32Mul FixedI32Dot Refl impossible
operationCodeInjective TensorF32Mul FixedI32MatMul Refl impossible
operationCodeInjective TensorF32Mul TensorF32Relu Refl impossible
operationCodeInjective TensorF32Mul TensorF32Relu6 Refl impossible
operationCodeInjective TensorF32Mul TensorF32MatMul Refl impossible
operationCodeInjective TensorF32Mul TensorF32Add Refl impossible

||| Contrapositive form for consumers: different operations cannot share a
||| numeric discriminant.
public export
differentOperationsHaveDifferentCodes : (left, right : Operation) ->
                                         Not (left = right) ->
                                         Not (operationCode left = operationCode right)
differentOperationsHaveDifferentCodes left right operationsDiffer codesEqual =
  operationsDiffer (operationCodeInjective left right codesEqual)

public export
operationCodesDistinct : Not (operationCode FixedI32Dot = operationCode FixedI32MatMul)
operationCodesDistinct = differentOperationsHaveDifferentCodes
  FixedI32Dot FixedI32MatMul (\case Refl impossible)

public export
reluCodesDistinct : Not (operationCode TensorF32Relu = operationCode TensorF32Relu6)
reluCodesDistinct = differentOperationsHaveDifferentCodes
  TensorF32Relu TensorF32Relu6 (\case Refl impossible)

public export
fixedAndTensorCodesDistinct : Not (operationCode FixedI32MatMul = operationCode TensorF32Relu)
fixedAndTensorCodesDistinct = differentOperationsHaveDifferentCodes
  FixedI32MatMul TensorF32Relu (\case Refl impossible)

public export
binaryTensorCodesDistinct : Not (operationCode TensorF32MatMul = operationCode TensorF32Add)
binaryTensorCodesDistinct = differentOperationsHaveDifferentCodes
  TensorF32MatMul TensorF32Add (\case Refl impossible)

public export
pointwiseBinaryCodesDistinct : Not (operationCode TensorF32Add = operationCode TensorF32Mul)
pointwiseBinaryCodesDistinct = differentOperationsHaveDifferentCodes
  TensorF32Add TensorF32Mul (\case Refl impossible)

private
div8_56 : Divides 8 56
div8_56 = MkDivides 7 Refl

private
div8_0 : Divides 8 0
div8_0 = MkDivides 0 Refl

private
div8_32 : Divides 8 32
div8_32 = MkDivides 4 Refl

private
div8_40 : Divides 8 40
div8_40 = MkDivides 5 Refl

private
div8_48 : Divides 8 48
div8_48 = MkDivides 6 Refl

private
div4_4 : Divides 4 4
div4_4 = MkDivides 1 Refl

private
div4_8 : Divides 4 8
div4_8 = MkDivides 2 Refl

private
div4_12 : Divides 4 12
div4_12 = MkDivides 3 Refl

private
div4_16 : Divides 4 16
div4_16 = MkDivides 4 Refl

private
div4_20 : Divides 4 20
div4_20 = MkDivides 5 Refl

private
div4_24 : Divides 4 24
div4_24 = MkDivides 6 Refl

private
div4_28 : Divides 4 28
div4_28 = MkDivides 7 Refl

private
div4_32 : Divides 4 32
div4_32 = MkDivides 8 Refl

private
div2_0 : Divides 2 0
div2_0 = MkDivides 0 Refl

private
div2_2 : Divides 2 2
div2_2 = MkDivides 1 Refl

private
div2_4 : Divides 2 4
div2_4 = MkDivides 2 Refl

private
div2_6 : Divides 2 6
div2_6 = MkDivides 3 Refl

public export
requestLayout : StructLayout
requestLayout = MkStructLayout
  [ MkField "abi_major" 0 2 2
  , MkField "abi_minor" 2 2 2
  , MkField "operation_major" 4 2 2
  , MkField "operation_minor" 6 2 2
  , MkField "operation" 8 4 4
  , MkField "lane" 12 4 4
  , MkField "minimum_support" 16 4 4
  , MkField "minimum_determinism" 20 4 4
  , MkField "layout" 24 4 4
  , MkField "reserved" 28 4 4
  , MkField "dim0" 32 8 8
  , MkField "dim1" 40 8 8
  , MkField "dim2" 48 8 8
  ] 56 8 {aligned = div8_56}

public export
requestLayoutValid : CABICompliant Abi.Accelerator.requestLayout
requestLayoutValid = CABIOk Abi.Accelerator.requestLayout
  (ConsField _ _ div2_0
  (ConsField _ _ div2_2
  (ConsField _ _ div2_4
  (ConsField _ _ div2_6
  (ConsField _ _ div4_8
  (ConsField _ _ div4_12
  (ConsField _ _ div4_16
  (ConsField _ _ div4_20
  (ConsField _ _ div4_24
  (ConsField _ _ div4_28
  (ConsField _ _ div8_32
  (ConsField _ _ div8_40
  (ConsField _ _ div8_48 NoFields)))))))))))))
  (ConsBound _ _ (ltePlusRight 2 54)
  (ConsBound _ _ (ltePlusRight 4 52)
  (ConsBound _ _ (ltePlusRight 6 50)
  (ConsBound _ _ (ltePlusRight 8 48)
  (ConsBound _ _ (ltePlusRight 12 44)
  (ConsBound _ _ (ltePlusRight 16 40)
  (ConsBound _ _ (ltePlusRight 20 36)
  (ConsBound _ _ (ltePlusRight 24 32)
  (ConsBound _ _ (ltePlusRight 28 28)
  (ConsBound _ _ (ltePlusRight 32 24)
  (ConsBound _ _ (ltePlusRight 40 16)
  (ConsBound _ _ (ltePlusRight 48 8)
  (ConsBound _ _ (ltePlusRight 56 0) NoBounds)))))))))))))

public export
bufferLayout : StructLayout
bufferLayout = MkStructLayout
  [ MkField "data" 0 8 8
  , MkField "len" 8 8 8
  ] 16 8 {aligned = div8_16}

public export
bufferLayoutValid : CABICompliant Abi.Accelerator.bufferLayout
bufferLayoutValid = CABIOk Abi.Accelerator.bufferLayout
  (ConsField _ _ div8_0 (ConsField _ _ div8_8 NoFields))
  (ConsBound _ _ (ltePlusRight 8 8)
  (ConsBound _ _ (ltePlusRight 16 0) NoBounds))

public export
capabilityLayout : StructLayout
capabilityLayout = MkStructLayout
  [ MkField "abi_major" 0 2 2
  , MkField "abi_minor" 2 2 2
  , MkField "operation_major" 4 2 2
  , MkField "operation_minor" 6 2 2
  , MkField "operation" 8 4 4
  , MkField "support" 12 4 4
  , MkField "determinism" 16 4 4
  , MkField "backend_id" 20 4 4
  , MkField "device_class" 24 4 4
  , MkField "flags" 28 4 4
  ] 32 4 {aligned = div4_32}

public export
capabilityLayoutValid : CABICompliant Abi.Accelerator.capabilityLayout
capabilityLayoutValid = CABIOk Abi.Accelerator.capabilityLayout
  (ConsField _ _ div2_0
  (ConsField _ _ div2_2
  (ConsField _ _ div2_4
  (ConsField _ _ div2_6
  (ConsField _ _ div4_8
  (ConsField _ _ div4_12
  (ConsField _ _ div4_16
  (ConsField _ _ div4_20
  (ConsField _ _ div4_24
  (ConsField _ _ div4_28 NoFields))))))))))
  (ConsBound _ _ (ltePlusRight 2 30)
  (ConsBound _ _ (ltePlusRight 4 28)
  (ConsBound _ _ (ltePlusRight 6 26)
  (ConsBound _ _ (ltePlusRight 8 24)
  (ConsBound _ _ (ltePlusRight 12 20)
  (ConsBound _ _ (ltePlusRight 16 16)
  (ConsBound _ _ (ltePlusRight 20 12)
  (ConsBound _ _ (ltePlusRight 24 8)
  (ConsBound _ _ (ltePlusRight 28 4)
  (ConsBound _ _ (ltePlusRight 32 0) NoBounds))))))))))

public export
evidenceLayout : StructLayout
evidenceLayout = MkStructLayout
  [ MkField "abi_major" 0 2 2
  , MkField "abi_minor" 2 2 2
  , MkField "operation_major" 4 2 2
  , MkField "operation_minor" 6 2 2
  , MkField "operation" 8 4 4
  , MkField "backend_id" 12 4 4
  , MkField "support" 16 4 4
  , MkField "determinism" 20 4 4
  ] 24 4 {aligned = div4_24}

public export
evidenceLayoutValid : CABICompliant Abi.Accelerator.evidenceLayout
evidenceLayoutValid = CABIOk Abi.Accelerator.evidenceLayout
  (ConsField _ _ div2_0
  (ConsField _ _ div2_2
  (ConsField _ _ div2_4
  (ConsField _ _ div2_6
  (ConsField _ _ div4_8
  (ConsField _ _ div4_12
  (ConsField _ _ div4_16
  (ConsField _ _ div4_20 NoFields))))))))
  (ConsBound _ _ (ltePlusRight 2 22)
  (ConsBound _ _ (ltePlusRight 4 20)
  (ConsBound _ _ (ltePlusRight 6 18)
  (ConsBound _ _ (ltePlusRight 8 16)
  (ConsBound _ _ (ltePlusRight 12 12)
  (ConsBound _ _ (ltePlusRight 16 8)
  (ConsBound _ _ (ltePlusRight 20 4)
  (ConsBound _ _ (ltePlusRight 24 0) NoBounds))))))))
