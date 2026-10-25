-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Executable entry point for the Enaction Engine proven-tests suite.

module EnactionEngineTests.Main

import EnactionEngineTests.AbiTests
import ProvenTests.Framework
import ProvenTests.Types
import System

%default total

private
suite : TestSuite
suite = MkTestSuite "Enaction Engine ABI conformance" (map toRunnable allAbiTests)

private
isPass : TestResult -> Bool
isPass Passed = True
isPass _ = False

private
printOutcome : (TestMetadata, TestResult) -> IO ()
printOutcome (metadata, result) =
  putStrLn $ "  [" ++ show (statusOf metadata.provenance) ++ "] "
          ++ metadata.test_id.test_name ++ ": " ++ show result

main : IO ()
main = do
  putStrLn "=== Enaction Engine ABI conformance: proven-tests-and-benches ==="
  results <- runSuite suite
  traverse_ printOutcome results
  let passed = length (filter (isPass . snd) results)
  putStrLn $ "=== " ++ show passed ++ "/" ++ show (length results) ++ " passed ==="
  if passed == length results
     then putStrLn "SUITE: PASS"
     else do
       putStrLn "SUITE: FAIL"
       exitFailure
