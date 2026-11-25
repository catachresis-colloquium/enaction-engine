;; SPDX-License-Identifier: MPL-2.0
;; Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
;;
;; Tools for scripts/generate-accelerator-abi.sh, resolved at the Guix commit
;; pinned in build/channels.scm.
;;
;; Confirmed at that commit by reading gnu/packages/zig.scm: package `zig`
;; version 0.16.0 (variable `zig-0.16`), matching the Zig pin in mise.toml and
;; .tool-versions.
;;
;; NOT CONFIRMED: Idris2. No gnu/packages/idris*.scm file exists at that commit
;; and a text search of gnu/packages did not finish, so there is no known Guix
;; package for Idris2 0.7.0 (the version `.tool-versions` lists, commented out).
;; It is left out on purpose rather than guessed. This manifest is therefore
;; INCOMPLETE: the owner must supply an Idris2 0.7.x package (a local package
;; definition or another channel) and add it below before the generator can run.

(specifications->manifest
 (list "zig@0.16"
       ;; "idris2@0.7"  ; <- not confirmed to exist in Guix; see the note above.
       "bash"
       "coreutils"
       "git"))
