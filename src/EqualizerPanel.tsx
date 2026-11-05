import { useCallback, useEffect, useRef, useState } from "react";
import type { AudioController } from "./audio";
import {
  EQ_BANDS,
  EQ_PRESETS,
  MAX_CROSSFADE_MS,
  MAX_GAIN_DB,
  MIN_GAIN_DB,
  applyPreset,
  defaultProcessing,
  detectPreset,
  formatGain,
  loadProcessing,
  normalizeEqualizerSettings,
  removeCustomPreset,
  saveCustomPreset,
  saveProcessing,
  supportsBand,
  withBandGain,
  type EqualizerSettings,
  type ProcessingDiagnostics,
  type ReplayGainMode,
} from "./equalizer";
import "./Equalizer.css";

function formatBalance(value: number): string {
  if (Math.abs(value) < 0.01) return "Centro";
  const percent = Math.round(Math.abs(value) * 100);
  return value < 0 ? `Izquierda ${percent}%` : `Derecha ${percent}%`;
}

function formatCrossfade(value: number): string {
  if (value <= 0) return "Desactivado";
  return value % 1000 === 0
    ? `${value / 1000} s`
    : `${(value / 1000).toFixed(2)} s`;
}

export function Equalizer({ audio }: { audio: AudioController }) {
  const [settings, setSettings] = useState<EqualizerSettings>(
    defaultProcessing.settings,
  );
  const [diagnostics, setDiagnostics] = useState<ProcessingDiagnostics>(
    defaultProcessing.diagnostics,
  );
  const [ready, setReady] = useState(!audio.available);
  const [customName, setCustomName] = useState("");
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const lastSaved = useRef<EqualizerSettings | null>(null);
  const latest = useRef(settings);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());

  const persist = useCallback((next: EqualizerSettings) => {
    const request = saveQueue.current.then(() => saveProcessing(next));
    saveQueue.current = request.then(
      () => undefined,
      () => undefined,
    );
    return request;
  }, []);

  useEffect(() => {
    latest.current = settings;
  }, [settings]);

  useEffect(() => {
    let disposed = false;
    if (!audio.available) {
      lastSaved.current = defaultProcessing.settings;
      return () => {
        disposed = true;
      };
    }
    void loadProcessing()
      .then((snapshot) => {
        if (disposed) return;
        if (snapshot) {
          setSettings(snapshot.settings);
          setDiagnostics(snapshot.diagnostics);
          lastSaved.current = snapshot.settings;
        } else {
          lastSaved.current = defaultProcessing.settings;
        }
        setReady(true);
      })
      .catch((reason) => {
        if (disposed) return;
        setError(String(reason));
        lastSaved.current = defaultProcessing.settings;
        setReady(true);
      });
    return () => {
      disposed = true;
    };
  }, [audio.available]);

  useEffect(() => {
    if (!ready || !audio.available) return;
    if (lastSaved.current === settings) return;
    const timer = window.setTimeout(() => {
      void persist(settings)
        .then((snapshot) => {
          lastSaved.current = settings;
          if (snapshot) setDiagnostics(snapshot.diagnostics);
          setError("");
        })
        .catch((reason) => setError(String(reason)));
    }, 280);
    return () => window.clearTimeout(timer);
  }, [audio.available, persist, ready, settings]);

  useEffect(
    () => () => {
      if (lastSaved.current && latest.current !== lastSaved.current)
        void persist(latest.current).catch(() => undefined);
    },
    [persist],
  );

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(""), 4000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  function edit(change: (current: EqualizerSettings) => EqualizerSettings) {
    setSettings((current) => {
      const next = normalizeEqualizerSettings(change(current));
      next.preset = detectPreset(next);
      return next;
    });
    setNotice("");
  }

  function choosePreset(id: string) {
    if (id === "custom") return;
    edit((current) => applyPreset(current, id));
  }

  function resetBands() {
    edit((current) => ({
      ...current,
      preset: "flat",
      gains: EQ_BANDS.map(() => 0),
      preampDb: 0,
      balance: 0,
      replayGain: "off",
      normalization: false,
      crossfadeMs: 0,
      clipGuard: true,
    }));
    setNotice(
      "Curva plana restaurada. Los presets personalizados se conservan.",
    );
  }

  function storeCustomPreset(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      setSettings(saveCustomPreset(settings, customName));
      setNotice(`Preset «${customName.trim()}» guardado.`);
      setCustomName("");
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  function deleteCustomPreset(name: string) {
    setSettings(removeCustomPreset(settings, name));
    setNotice(`Preset «${name}» eliminado.`);
  }

  const sampleRate = diagnostics.sampleRate ?? audio.state.sampleRate;
  const supportedCount = EQ_BANDS.filter((band) =>
    supportsBand(band.hz, sampleRate),
  ).length;
  const presetOptions = [
    ...EQ_PRESETS.map((preset) => ({ id: preset.id, label: preset.label })),
    ...settings.customPresets.map((preset) => ({
      id: `custom:${preset.name}`,
      label: preset.name,
    })),
  ];
  const presetValue = presetOptions.some(
    (option) => option.id === settings.preset,
  )
    ? settings.preset
    : detectPreset(settings);
  const status = settings.enabled
    ? "Procesando en vivo · los cambios se aplican sin cortar la reproducción"
    : "Procesamiento desactivado · el audio pasa sin cambios";

  return (
    <section className="equalizer" aria-labelledby="equalizer-heading">
      <header className="eq-hero">
        <div className="eq-hero-copy">
          <p className="eq-eyebrow">Procesamiento de audio</p>
          <h2 id="equalizer-heading">Ecualizador de diez bandas</h2>
          <p>
            Ajusta el balance tonal, la ganancia de entrada y la dinámica sin
            recargar la pista. Los ajustes se guardan automáticamente.
          </p>
        </div>
        <label className="eq-master">
          <input
            type="checkbox"
            checked={settings.enabled}
            disabled={!audio.available}
            onChange={(event) =>
              edit((current) => ({
                ...current,
                enabled: event.target.checked,
              }))
            }
          />
          <span>
            <strong>{settings.enabled ? "Activado" : "Desactivado"}</strong>
            <small>{status}</small>
          </span>
        </label>
      </header>

      {!audio.available && (
        <p className="inline-note">
          El ecualizador está disponible en la aplicación de escritorio. Los
          controles se muestran como referencia.
        </p>
      )}

      <fieldset
        className="eq-body"
        disabled={!audio.available}
        aria-busy={!ready}
      >
        <legend className="sr-only">Controles del ecualizador</legend>

        <div className="eq-preset-bar">
          <label className="eq-select">
            <span>Preset</span>
            <select
              value={presetValue}
              onChange={(event) => choosePreset(event.target.value)}
            >
              {presetOptions.map((option) => (
                <option value={option.id} key={option.id}>
                  {option.label}
                </option>
              ))}
              {presetValue === "custom" && (
                <option value="custom">Personalizado</option>
              )}
            </select>
          </label>
          <button className="button" type="button" onClick={resetBands}>
            Restablecer plano
          </button>
          <span className="eq-status" role="status">
            {settings.enabled ? "Procesando" : "En pausa"}
          </span>
        </div>

        <section className="eq-section" aria-labelledby="eq-bands-title">
          <div className="eq-section-heading">
            <h3 id="eq-bands-title">Curva de frecuencias</h3>
            <span>
              {MIN_GAIN_DB} dB a +{MAX_GAIN_DB} dB · {supportedCount} de{" "}
              {EQ_BANDS.length} bandas disponibles
            </span>
          </div>
          <div
            className="eq-bands"
            role="group"
            aria-label="Bandas del ecualizador"
          >
            {EQ_BANDS.map((band, index) => {
              const supported = supportsBand(band.hz, sampleRate);
              return (
                <div
                  className={`eq-band ${supported ? "" : "unsupported"}`}
                  key={band.hz}
                >
                  <span className="eq-band-gain">
                    {formatGain(settings.gains[index])}
                  </span>
                  <input
                    type="range"
                    min={MIN_GAIN_DB}
                    max={MAX_GAIN_DB}
                    step={0.5}
                    value={settings.gains[index]}
                    disabled={!supported}
                    aria-label={`Ganancia de la banda ${band.hz} hercios`}
                    aria-valuetext={formatGain(settings.gains[index])}
                    onChange={(event) =>
                      edit((current) =>
                        withBandGain(
                          current,
                          index,
                          Number(event.target.value),
                        ),
                      )
                    }
                  />
                  <span className="eq-band-hz">{band.label}</span>
                  <small>{band.hz < 1000 ? "Hz" : "kHz"}</small>
                </div>
              );
            })}
          </div>
          {sampleRate !== null && supportedCount < EQ_BANDS.length && (
            <p className="eq-hint">
              Las bandas atenuadas superan el límite de Nyquist para{" "}
              {(sampleRate / 1000).toFixed(1)} kHz y se omiten para evitar
              aliasing.
            </p>
          )}
        </section>

        <div className="eq-control-grid">
          <section className="eq-card" aria-labelledby="eq-preamp-title">
            <h3 id="eq-preamp-title">Preamplificación</h3>
            <label className="eq-range">
              <span>
                Ganancia
                <output>{formatGain(settings.preampDb)}</output>
              </span>
              <input
                type="range"
                min={MIN_GAIN_DB}
                max={MAX_GAIN_DB}
                step={0.5}
                value={settings.preampDb}
                onChange={(event) =>
                  edit((current) => ({
                    ...current,
                    preampDb: Number(event.target.value),
                  }))
                }
              />
            </label>
            <p className="eq-hint">
              Compensa la curva antes del limitador. Ganancia efectiva:{" "}
              {formatGain(diagnostics.effectiveGainDb)}.
            </p>
          </section>

          <section className="eq-card" aria-labelledby="eq-balance-title">
            <h3 id="eq-balance-title">Balance</h3>
            <label className="eq-range">
              <span>
                Canal
                <output>{formatBalance(settings.balance)}</output>
              </span>
              <input
                type="range"
                min={-1}
                max={1}
                step={0.01}
                value={settings.balance}
                aria-valuetext={formatBalance(settings.balance)}
                onChange={(event) =>
                  edit((current) => ({
                    ...current,
                    balance: Number(event.target.value),
                  }))
                }
              />
            </label>
            <button
              className="text-button"
              type="button"
              onClick={() => edit((current) => ({ ...current, balance: 0 }))}
            >
              Centrar
            </button>
          </section>

          <section className="eq-card" aria-labelledby="eq-replaygain-title">
            <h3 id="eq-replaygain-title">ReplayGain y normalización</h3>
            <label className="eq-select">
              <span>ReplayGain</span>
              <select
                value={settings.replayGain}
                onChange={(event) =>
                  edit((current) => ({
                    ...current,
                    replayGain: event.target.value as ReplayGainMode,
                  }))
                }
              >
                <option value="off">Desactivado</option>
                <option value="track">Por pista</option>
                <option value="album">Por álbum</option>
              </select>
            </label>
            <label className="eq-check">
              <input
                type="checkbox"
                checked={settings.normalization}
                onChange={(event) =>
                  edit((current) => ({
                    ...current,
                    normalization: event.target.checked,
                  }))
                }
              />
              <span>
                Limitar la ganancia estática si el pico medido supera −1 dBFS
              </span>
            </label>
            <p className="eq-hint">
              ReplayGain aplicado: {formatGain(diagnostics.replayGainDb)}. La
              normalización usa el pico ReplayGain de la pista o del álbum.
            </p>
          </section>

          <section className="eq-card" aria-labelledby="eq-crossfade-title">
            <h3 id="eq-crossfade-title">Fundido entre pistas</h3>
            <label className="eq-range">
              <span>
                Duración
                <output>{formatCrossfade(settings.crossfadeMs)}</output>
              </span>
              <input
                type="range"
                min={0}
                max={MAX_CROSSFADE_MS}
                step={250}
                value={settings.crossfadeMs}
                aria-valuetext={formatCrossfade(settings.crossfadeMs)}
                onChange={(event) =>
                  edit((current) => ({
                    ...current,
                    crossfadeMs: Number(event.target.value),
                  }))
                }
              />
            </label>
            <p className="eq-hint">
              Fundido de entrada y salida en los extremos de cada pista. No
              solapa dos canciones, por lo que la reproducción sin cortes se
              mantiene.
            </p>
          </section>

          <section className="eq-card" aria-labelledby="eq-dynamics-title">
            <h3 id="eq-dynamics-title">Protección de dinámica</h3>
            <label className="eq-check">
              <input
                type="checkbox"
                checked={settings.clipGuard}
                onChange={(event) =>
                  edit((current) => ({
                    ...current,
                    clipGuard: event.target.checked,
                  }))
                }
              />
              <span>Reservar margen automático y limitar picos suavemente</span>
            </label>
            <p className="eq-hint">
              {settings.clipGuard
                ? `Margen calculado: ${formatGain(diagnostics.headroomDb)}.`
                : "Sin margen automático: vigila la ganancia para evitar saturación."}
            </p>
          </section>
        </div>

        <section className="eq-custom" aria-labelledby="eq-custom-title">
          <div className="eq-section-heading">
            <h3 id="eq-custom-title">Presets personalizados</h3>
            <span>{settings.customPresets.length} guardados</span>
          </div>
          <form className="eq-custom-form" onSubmit={storeCustomPreset}>
            <input
              value={customName}
              maxLength={60}
              placeholder="Nombre del preset"
              aria-label="Nombre del preset personalizado"
              onChange={(event) => setCustomName(event.target.value)}
            />
            <button
              className="button"
              type="submit"
              disabled={!customName.trim()}
            >
              Guardar curva actual
            </button>
          </form>
          {settings.customPresets.length > 0 ? (
            <ul className="eq-custom-list">
              {settings.customPresets.map((preset) => (
                <li key={preset.name}>
                  <button
                    className="eq-chip"
                    type="button"
                    onClick={() => choosePreset(`custom:${preset.name}`)}
                  >
                    {preset.name}
                  </button>
                  <button
                    className="text-button"
                    type="button"
                    aria-label={`Eliminar el preset ${preset.name}`}
                    onClick={() => deleteCustomPreset(preset.name)}
                  >
                    Eliminar
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="eq-hint">
              Ajusta las bandas y guarda la curva para reutilizarla.
            </p>
          )}
        </section>

        <section className="eq-diagnostics" aria-labelledby="eq-diag-title">
          <div className="eq-section-heading">
            <h3 id="eq-diag-title">Diagnóstico del procesamiento</h3>
            <span>Solo lectura</span>
          </div>
          <dl className="eq-stats">
            <div>
              <dt>Muestreo</dt>
              <dd>
                {sampleRate
                  ? `${(sampleRate / 1000).toFixed(1)} kHz`
                  : "Sin pista"}
              </dd>
            </div>
            <div>
              <dt>Bandas activas</dt>
              <dd>
                {diagnostics.activeBands} / {EQ_BANDS.length}
              </dd>
            </div>
            <div>
              <dt>Ganancia efectiva</dt>
              <dd>{formatGain(diagnostics.effectiveGainDb)}</dd>
            </div>
            <div>
              <dt>Margen</dt>
              <dd>{formatGain(diagnostics.headroomDb)}</dd>
            </div>
            <div>
              <dt>Limitador</dt>
              <dd>
                {!settings.clipGuard
                  ? "Desactivado"
                  : diagnostics.limiterEngaged
                    ? "Actuando"
                    : "Armado"}
              </dd>
            </div>
            <div>
              <dt>ReplayGain</dt>
              <dd>{formatGain(diagnostics.replayGainDb)}</dd>
            </div>
          </dl>
        </section>
      </fieldset>

      {notice && (
        <p className="inline-note" role="status">
          {notice}
        </p>
      )}
      {error && (
        <p className="error-note" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
