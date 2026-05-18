;; BayArena Lighting Report
;; Run: cargo run -p bimifc-lisp -- --ifc crates/bimifc-viewer/ifc/bayarena_lighting.ifc crates/bimifc-lisp/examples/bayarena_report.lsp

(princ "=== BayArena Lighting Report ===\n")

;; Metadata
(setq meta (ifc-metadata))
(princ (strcat "Schema: " (cadr (assoc "schema" meta)) "\n"))
(princ (strcat "System: " (cadr (assoc "system" meta)) "\n"))
(princ (strcat "Total entities: " (itoa (ifc-entity-count)) "\n"))
(princ "\n")

;; Storeys
(princ "--- Storeys ---\n")
(foreach s (ifc-storeys)
  (princ (strcat "  " (cadr s)
         " (elev " (rtos (caddr s) 2 1) "m"
         ", " (itoa (length (ifc-elements-in-storey (car s)))) " elements)\n"))
)
(princ "\n")

;; Light fixtures
(setq fixtures (ifc-entities-by-type "IfcLightFixture"))
(princ (strcat "--- Light Fixtures: " (itoa (length fixtures)) " ---\n"))

;; Count by direction (first 10 as sample)
(setq count 0)
(foreach f fixtures
  (if (< count 5)
    (progn
      (princ (strcat "  #" (itoa f) " " (ifc-entity-name f) "\n"))
      (setq count (1+ count))
    )
  )
)
(if (> (length fixtures) 5)
  (princ (strcat "  ... and " (itoa (- (length fixtures) 5)) " more\n"))
)
(princ "\n")

;; Other element types
(princ "--- Element Summary ---\n")
(setq slabs (ifc-entities-by-type "IfcSlab"))
(princ (strcat "  Slabs: " (itoa (length slabs)) "\n"))
(setq roofs (ifc-entities-by-type "IfcRoof"))
(princ (strcat "  Roofs: " (itoa (length roofs)) "\n"))
(setq proxies (ifc-entities-by-type "IfcBuildingElementProxy"))
(princ (strcat "  Proxies: " (itoa (length proxies)) "\n"))
(setq assemblies (ifc-entities-by-type "IfcElementAssembly"))
(princ (strcat "  Assemblies: " (itoa (length assemblies)) "\n"))

(princ "\nDone.\n")
