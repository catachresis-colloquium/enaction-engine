; SPDX-License-Identifier: MPL-2.0
;; guix.scm — GNU Guix package definition for enaction-engine
;; Usage: guix shell -f guix.scm

(use-modules (guix packages)
             (guix build-system gnu)
             (guix licenses))

(package
  (name "enaction-engine")
  (version "0.1.0")
  (source #f)
  (build-system gnu-build-system)
  (synopsis "Deterministic, type-safe runtime facilities for situated agents and worlds")
  (description "enaction-engine — part of the metadatastician ecosystem.")
  (home-page "https://github.com/metadatastician/enaction-engine")
  ;; Interim: the generated ABI outputs still carry AGPL-3.0-or-later until they
  ;; are regenerated (docs/legal/RELICENSE-2026-11-24.adoc); drop the second entry then.
  (license (list ((@@ (guix licenses) license) "MPL-2.0"
                  "https://www.mozilla.org/en-US/MPL/2.0/")
                 ((@@ (guix licenses) license) "AGPL-3.0-or-later"
                  "https://www.gnu.org/licenses/agpl-3.0.html"))))
