;; SPDX-License-Identifier: AGPL-3.0-or-later
;;
;; Development toolchain for enaction-engine.
;; idris2 comes from the estate channel (see channels.scm); the rest from Guix.
(specifications->manifest
 '("idris2@0.7.0"
   "zig@0.16"
   "rust"
   "rust:cargo"
   "just"
   "gcc-toolchain"
   "git"))
