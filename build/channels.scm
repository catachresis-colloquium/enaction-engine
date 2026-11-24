;; SPDX-License-Identifier: MPL-2.0
;; Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
;;
;; Pinned Guix channel set for regenerating the accelerator ABI
;; (scripts/generate-accelerator-abi.sh). One explicit Guix commit, so the same
;; toolchain is resolved every time:
;;
;;   guix time-machine -C build/channels.scm -- shell -m build/manifest.scm -- \
;;     ./scripts/generate-accelerator-abi.sh
;;   guix time-machine -C build/channels.scm -- shell -m build/manifest.scm -- \
;;     ./scripts/generate-accelerator-abi.sh --check
;;
;; The commit is the head of master of https://codeberg.org/guix/guix.git read on
;; 2026-11-24. The introduction below is the one published in the Guix manual
;; for the `guix` channel; the commit it names exists in that repository.
;; This file has NOT been exercised with `guix` (not installed where it was
;; written): confirm with `guix time-machine -C build/channels.scm -- describe`.

(list (channel
        (name 'guix)
        (url "https://codeberg.org/guix/guix.git")
        (commit "37d0e8124fe388929d66022669d568d95ecf945b")
        (introduction
         (make-channel-introduction
          "9edb3f66fd807b096b48283debdcddccfea34bad"
          (openpgp-fingerprint
           "BBB0 2DDF 2CEA F6A8 0D1D  E643 A2A0 6DF2 A33A 54FA")))))
