interface ShortcutsDialogProps {
  open: boolean;
  onClose: () => void;
}

const SHORTCUTS: Array<[string, string]> = [
  ["Espacio", "Reproducir / pausar"],
  ["F9", "Mezclar / siguiente pista"],
  ["S", "Pausa al final de la pista actual"],
  ["T", "Talkover (-14 dB)"],
  ["M", "Micrófono en aire"],
  ["F1 – F8", "Disparar pads del cart wall"],
  ["?", "Abrir esta ayuda"],
  ["Esc", "Cerrar diálogos"],
];

export function ShortcutsDialog({ open, onClose }: ShortcutsDialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="modal-backdrop" role="presentation" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcuts-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="modal__header">
          <h2 id="shortcuts-title">Atajos de operación</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Cerrar ayuda">
            ✕
          </button>
        </div>
        <dl className="shortcuts">
          {SHORTCUTS.map(([key, description]) => (
            <div key={key} className="shortcuts__row">
              <dt>
                <kbd>{key}</kbd>
              </dt>
              <dd>{description}</dd>
            </div>
          ))}
        </dl>
        <p className="modal__footnote">
          Los atajos se disparan fuera de campos de texto. En la próxima fase serán configurables
          desde Preferencias.
        </p>
      </div>
    </div>
  );
}
