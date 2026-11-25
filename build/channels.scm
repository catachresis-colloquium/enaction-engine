;; SPDX-License-Identifier: AGPL-3.0-or-later
;;
;; Pinned channels for the enaction-engine toolchain (Idris2 0.7.0, Zig).
;; Use with:  guix time-machine -C build/channels.scm -- shell -m build/manifest.scm
;;
;; Both channels carry their *introduction*, so `guix pull`/`time-machine`
;; authenticates every commit from the pinned one forward (fail closed).
;;
;; The estate pin moves only deliberately, after `guix git authenticate` passes.
;; Tested on Guix 1.4.0 only. Idris2 build is not bit-reproducible.
(list
 (channel
  (name 'guix)
  (url "https://git.savannah.gnu.org/git/guix.git")
  (introduction
   (make-channel-introduction
    "9edb3f66fd807b096b48283debdcddccfea34bad"
    (openpgp-fingerprint
     "BBB0 2DDF 2CEA F6A8 0D1D  E643 A2A0 6DF2 A33A 54FA"))))
 (channel
  (name 'estate)
  (url "https://github.com/hyperpolymath/guix-channel")
  (branch "main")
  (commit "d4c84bde7a728a4a5bbc6bf865ed45e983d01c6d")
  (introduction
   (make-channel-introduction
    "4b91d37a4bbb4bc8c16de791eb5bd2241fb26f4b"
    (openpgp-fingerprint
     "E0B3 6C1B 28F6 E9E0 A460  3F47 FE6E 34E9 F311 0F92")))))
