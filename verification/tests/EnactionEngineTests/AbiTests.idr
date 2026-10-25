-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Enaction Engine ABI conformance suite using proven-tests-and-benches.

module EnactionEngineTests.AbiTests

import Abi.Accelerator
import Abi.Foreign
import Abi.Generate
import Abi.Layout
import Abi.Types
import Data.Bits
import Data.List
import Data.Maybe
import Data.String
import Data.Vect
import ProvenTests.Classification
import ProvenTests.Framework
import ProvenTests.Types

%default total

private
frameworkTypeSafety : TypeSafetyCertificate
frameworkTypeSafety =
  typeSafetyCert 6 "Idris2 dependent types" "Idris2 0.7.0"
    ["ProvenTests classification", "Enaction Engine ABI test runner"]

private
suiteTypeSafety : TypeSafetyCertificate
suiteTypeSafety =
  typeSafetyCert 6 "Idris2 dependent types" "Idris2 0.7.0"
    ["Abi.Types", "Abi.Layout", "Abi.Accelerator", "Abi.Generate"]

private
suiteFrameworkSafety : FrameworkSafetyProof
suiteFrameworkSafety =
  frameworkProof "proven-tests-and-benches"
    frameworkTypeSafety suiteTypeSafety
    "Pinned, type-safe runner; every runtime test carries an explicit fixture pair."

private
runtimeFixturePair : String -> String -> String -> String -> FixtureObligation
runtimeFixturePair silenceConstruction silenceReason firingConstruction defectClass =
  Fixtures (MkFixturePair
    (MkSilenceFixture (RuntimeConstructed silenceConstruction) silenceReason)
    (MkFiringFixture (RuntimeConstructed firingConstruction) defectClass))

private
classifiedTest : Nat -> TestCategory -> TestAspect -> String -> FixtureObligation -> IO TestResult -> ProvisionallyProvenTest
classifiedTest number category aspect description fixtures action =
  let baseMetadata =
        classifyProvisionallyProven
          (MkTestId "EnactionEngineTests.AbiTests" description number)
          description suiteFrameworkSafety suiteTypeSafety fixtures
      metadata = { category := Just category, aspect := Just aspect } baseMetadata
  in MkProvisionallyProvenTest metadata action

private
codesAreUnique : Eq a => List a -> Bool
codesAreUnique [] = True
codesAreUnique (code :: rest) = not (code `elem` rest) && codesAreUnique rest

private
operationCodes : List Bits32
operationCodes =
  map operationCode
    [ FixedI32Dot
    , FixedI32MatMul
    , TensorF32Relu
    , TensorF32Relu6
    , TensorF32MatMul
    , TensorF32Add
    , TensorF32Mul
    ]

private
statusCodes : List Bits32
statusCodes =
  map statusCode
    [ StatusOk
    , NullPointer
    , UnsupportedAbi
    , UnsupportedOperationVersion
    , UnknownOperation
    , InvalidLane
    , InvalidSupport
    , InvalidDeterminism
    , LayoutMismatch
    , LengthMismatch
    , DimensionOverflow
    , ArithmeticOverflow
    , InvalidReservedField
    , UnsupportedRequirement
    , IndexOutOfRange
    , AliasingViolation
    , NonFiniteInput
    ]

private
duplicateOperationFixture : List Bits32
duplicateOperationFixture = [1, 2, 3, 4, 5, 6, 6]

private
duplicateStatusFixture : List Bits32
duplicateStatusFixture = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15]

private
stableCodeTables : Bool
stableCodeTables =
     operationCodes == [1, 2, 3, 4, 5, 6, 7]
  && codesAreUnique operationCodes
  && map layoutCode [DotLayout, MatMulLayout, VectorLayout] == [1, 2, 3]
  && map laneCode [Authoritative, Advisory, RemoteJob] == [1, 2, 3]
  && map determinismCode [AdvisoryOnly, ToleranceBounded, CanonicalExact] == [1, 2, 3]
  && map supportCode
       [ Declared, Discoverable, Loadable, Runnable, Conformant, Resilient
       , Deterministic, Benchmarked, ProductionSupported
       ] == [1, 2, 3, 4, 5, 6, 7, 8, 9]
  && map deviceCode
       [Cpu, Gpu, Tpu, Npu, Dsp, Ppu, Math, Fpga, Vpu, Qpu, Crypto]
       == [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
  && statusCodes == [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
  && codesAreUnique statusCodes
  && not (codesAreUnique duplicateOperationFixture)
  && not (codesAreUnique duplicateStatusFixture)

private
versionTruthTable : Bool
versionTruthTable =
     acceptsVersion 1 0 1 0
  && not (acceptsVersion 1 0 1 1)
  && not (acceptsVersion 1 0 2 0)
  && not (acceptsVersion 1 0 0 0)
  && acceptsCurrentVersion
  && rejectsWrongMajor
  && rejectsNewerMinor

private
fieldAligned : Field -> Bool
fieldAligned field =
  field.alignment == 0 || field.offset `mod` field.alignment == 0

private
fieldWithinBounds : Nat -> Field -> Bool
fieldWithinBounds total field = field.offset + field.size <= total

private
layoutCalculations : Bool
layoutCalculations =
     map (\offset => paddingFor offset 8) [0, 4, 8, 13, 16, 24]
       == [0, 4, 0, 3, 0, 0]
  && map (\size => alignUp size 8) [0, 4, 8, 13, 16, 24]
       == [0, 8, 8, 16, 16, 24]
  && map (\size => alignUp size 0) [0, 7, 19] == [0, 7, 19]
  && exampleLayout.totalSize == 24
  && exampleLayout.alignment == 8
  && map (\field => field.offset) (toList exampleLayout.fields) == [0, 8, 16]
  && map (\field => field.size) (toList exampleLayout.fields) == [4, 8, 8]
  && all fieldAligned (toList exampleLayout.fields)
  && all (fieldWithinBounds exampleLayout.totalSize) (toList exampleLayout.fields)
  && not (fieldAligned (MkField "misaligned" 2 4 4))
  && not (fieldWithinBounds 4 (MkField "out-of-bounds" 4 4 4))

private
platformAndCLayouts : Bool
platformAndCLayouts =
  let pointerWidths = map ptrSize [Linux, MacOS, Windows, WASM, RISCV]
      linuxSizes = map (cSizeOf Linux) [CInt, CUInt, CLong, CULong, CPtrType]
      linuxAlignments = map (cAlignOf Linux) [CInt, CUInt, CLong, CULong, CPtrType]
  in pointerWidths == [64, 64, 64, 32, 64]
     && pointerWidths /= [64, 64, 64, 64, 64]
     && linuxSizes == [4, 4, 8, 8, 8]
     && linuxAlignments == [4, 4, 8, 8, 8]
     && linuxAlignments /= [4, 4, 4, 8, 8]

private
handleBoundary : Bool
handleBoundary =
  case (createHandle 0, createHandle 1) of
    (Nothing, Just _) => True
    _ => False

private
errorDescriptions : Bool
errorDescriptions =
  map errorDescription [Ok, Abi.Types.Error, InvalidParam, Busy]
    == ["Success", "Generic error", "Invalid parameter", "Library is busy"]

private
generatedDeclarationsAgree : Bool
generatedDeclarationsAgree =
     isInfixOf "#define ENACTION_ACCEL_ABI_MAJOR 1u" cHeader
  && isInfixOf "#define ENACTION_ACCEL_STATUS_NON_FINITE_INPUT 16u" cHeader
  && isInfixOf "pub const abi_major: u16 = 1;" zigDeclarations
  && isInfixOf "pub const status_non_finite_input: u32 = 16;" zigDeclarations
  && isInfixOf "pub const ABI_MAJOR: u16 = 1;" rustDeclarations
  && isInfixOf "pub const STATUS_NON_FINITE_INPUT: u32 = 16;" rustDeclarations

-- This is a compile-time negative fixture: if the ABI ever accepts a null data
-- pointer for a non-empty buffer, this declaration starts compiling and the
-- proven-tests package fails. The firing specimen is the impossible equality.
failing
  nonEmptyNullBufferCannotBeValid : bufferDataPointerValid NullData 1 = True
  nonEmptyNullBufferCannotBeValid = Refl

private
compileTimeNegativeFixture : ProvisionallyProvenTest
compileTimeNegativeFixture =
  let baseMetadata =
        classifyProvisionallyProven
          (MkTestId "EnactionEngineTests.AbiTests" "non-empty-null-buffer-negative-fixture" 0)
          "The typechecker rejects an invalid non-empty null-buffer proof."
          suiteFrameworkSafety suiteTypeSafety
          (CannotFailByDesign
            "The `failing` declaration above must remain uninhabited; compilation is the oracle.")
      metadata = { category := Just ProofRegressionTest, aspect := Just Safety } baseMetadata
  in MkProvisionallyProvenTest metadata (pure Passed)

public export
allAbiTests : List ProvisionallyProvenTest
allAbiTests =
  [ classifiedTest 1 ContractInvariantTest Safety "null pointer is valid only for an empty buffer"
      (runtimeFixturePair
        "construct NullData with length 0"
        "an empty buffer carries no missing payload"
        "construct NullData with length 1"
        "a non-empty buffer with a null data pointer")
      (assertEq
        (map (\(pointer, len) => bufferDataPointerValid pointer len)
          [(NullData, 0), (NullData, 1), (DataPresent, 0), (DataPresent, 1)])
        [True, False, True, True]
        "buffer pointer validity truth table changed")
  , classifiedTest 2 CompatibilityTest Interoperability "ABI version acceptance is major-exact and minor-bounded"
      (runtimeFixturePair
        "request the current ABI 1.0"
        "the current major/minor is supported"
        "request ABI 1.1 or a different major"
        "a newer minor or incompatible major")
      (assertTrue versionTruthTable "ABI version acceptance changed")
  , classifiedTest 3 CompatibilityTest Interoperability "operation and status discriminants remain stable and injective"
      (runtimeFixturePair
        "enumerate every operation and status constructor"
        "the canonical operation and status tables have unique codes"
        "introduce a duplicate operation or status discriminant"
        "two distinct ABI constructors share a numeric code")
      (assertTrue stableCodeTables "an ABI numeric code table drifted or collided")
  , classifiedTest 4 ContractInvariantTest Safety "alignment calculations and the proven request layout agree"
      (runtimeFixturePair
        "align the declared request fields and example layout"
        "field boundaries and final size satisfy their alignments"
        "place a field at an unaligned or out-of-bounds offset"
        "a struct field violates the C ABI layout contract")
      (assertTrue layoutCalculations "ABI alignment or example layout changed")
  , classifiedTest 5 CompatibilityTest Portability "platform pointer widths and Linux C layouts are explicit"
      (runtimeFixturePair
        "construct the five declared platform models"
        "the current Linux C layout follows the 64-bit target model"
        "substitute an incorrect pointer width or Linux C alignment"
        "a platform or C type size/alignment mapping drifted")
      (assertTrue platformAndCLayouts "platform pointer widths or C layouts changed")
  , classifiedTest 6 ContractInvariantTest Safety "opaque handles reject null and admit non-null pointers"
      (runtimeFixturePair
        "construct a zero-valued handle pointer"
        "zero is rejected before an opaque Handle is created"
        "construct a non-zero handle pointer"
        "a null pointer was admitted as a resource handle")
      (assertTrue handleBoundary "opaque handle creation boundary changed")
  , classifiedTest 7 UnitTest Functionality "FFI result descriptions are exhaustive and stable"
      (runtimeFixturePair
        "enumerate Ok, Error, InvalidParam and Busy"
        "every declared result has a meaningful message"
        "return an unmapped result description"
        "an FFI result code lost its stable human-readable description")
      (assertTrue errorDescriptions "FFI result descriptions changed")
  , classifiedTest 8 CompatibilityTest Interoperability "C, Zig and Rust declarations use the typed ABI constants"
      (runtimeFixturePair
        "render the typed ABI major and terminal status code"
        "generated declarations agree with the Idris2 authority"
        "change one generated language declaration independently"
        "C, Zig or Rust constants diverge from the Idris2 ABI")
      (assertTrue generatedDeclarationsAgree "generated ABI declarations drifted")
  , compileTimeNegativeFixture
  ]
