import { useEffect, useRef, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, Plus, Search, X, Star, Heart } from "lucide-react";
import type {
  ProviderPreset,
  ModelType,
  ModelCatalogEntry,
} from "../config/modelPresets";

interface Props {
  presets: ProviderPreset[];
  modelType: ModelType;
  currentApiBaseUrl: string;
  currentModel: string;
  lang: string;
  t: (key: string) => string;
  onSelect: (preset: ProviderPreset, model?: ModelCatalogEntry) => void;
}

export function ProviderPresetSelector({
  presets,
  modelType: _modelType,
  currentApiBaseUrl,
  currentModel,
  lang: _lang,
  t,
  onSelect,
}: Props) {
  const [pickerOpen, setPickerOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [expandedProvider, setExpandedProvider] = useState<string | null>(null);
  const addProviderButtonRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const currentPreset = useMemo(
    () =>
      presets.find(
        (preset) =>
          preset.apiBaseUrl === currentApiBaseUrl ||
          currentApiBaseUrl.includes(preset.apiBaseUrl.replace("https://", "").split("/")[0]),
      ),
    [presets, currentApiBaseUrl],
  );

  const filteredPresets = useMemo(() => {
    const query = search.trim().toLowerCase();
    const result = presets.filter(
      (preset) =>
        query.length === 0 ||
        preset.name.toLowerCase().includes(query) ||
        preset.models.some(
          (model) =>
            model.displayName.toLowerCase().includes(query) ||
            model.model.toLowerCase().includes(query),
        ),
    );

    if (currentPreset && !query) {
      return [currentPreset, ...result.filter((preset) => preset.name !== currentPreset.name)];
    }
    return result;
  }, [presets, search, currentPreset]);

  const closePicker = () => {
    setPickerOpen(false);
    setExpandedProvider(null);
    setSearch("");
    addProviderButtonRef.current?.focus();
  };

  useEffect(() => {
    if (!pickerOpen) return;

    searchInputRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closePicker();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;

      const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), a[href]',
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [pickerOpen]);

  const selectPreset = (preset: ProviderPreset, model?: ModelCatalogEntry) => {
    onSelect(preset, model);
    closePicker();
  };

  return (
    <div className="provider-preset-selector">
      <div className="provider-preset-summary">
        <div className="provider-preset-summary-main">
          <span
            className="provider-preset-icon"
            style={{ backgroundColor: currentPreset?.iconColor || undefined }}
          >
            {currentPreset?.name.charAt(0) || "?"}
          </span>
          <div className="provider-preset-summary-info">
            <span className="provider-preset-summary-label">{t("settings.provider_selected")}</span>
            <strong className="provider-preset-summary-name">
              {currentPreset?.name || t("settings.custom_provider")}
            </strong>
            <span className="provider-preset-summary-model">
              {currentModel || currentApiBaseUrl || t("settings.provider_not_configured")}
            </span>
          </div>
        </div>
        <button
          type="button"
          ref={addProviderButtonRef}
          className="btn btn-sm btn-secondary provider-preset-add"
          onClick={() => setPickerOpen(true)}
        >
          <Plus size={13} />
          {currentPreset ? t("settings.change_provider") : t("settings.add_provider")}
        </button>
      </div>

      {pickerOpen && (
        <div
          className="provider-preset-dialog-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closePicker();
          }}
        >
          <div
            ref={dialogRef}
            className="provider-preset-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="provider-preset-dialog-title"
            tabIndex={-1}
          >
            <div className="provider-preset-dialog-head">
              <div>
                <h4 id="provider-preset-dialog-title">{t("settings.provider_picker_title")}</h4>
                <p>{t("settings.provider_picker_lead")}</p>
              </div>
              <button
                type="button"
                className="provider-preset-dialog-close"
                onClick={closePicker}
                aria-label={t("settings.close_provider_picker")}
                title={t("settings.close_provider_picker")}
              >
                <X size={16} />
              </button>
            </div>

            <div className="provider-preset-search">
              <Search size={14} />
              <input
                autoFocus
                type="search"
                placeholder={t("settings.search_providers")}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </div>

            <div className="provider-preset-list">
              {filteredPresets.length === 0 ? (
                <p className="provider-preset-empty">{t("settings.no_matching_providers")}</p>
              ) : (
                filteredPresets.map((preset) => {
                  const expanded = expandedProvider === preset.name;
                  return (
                    <div key={preset.name} className="provider-preset-item">
                      <div className="provider-preset-header-row">
                        <button
                          type="button"
                          className="provider-preset-header"
                          onClick={() => setExpandedProvider(expanded ? null : preset.name)}
                          aria-expanded={expanded}
                        >
                          <span
                            className="provider-preset-icon"
                            style={{ backgroundColor: preset.iconColor || undefined }}
                          >
                            {preset.name.charAt(0)}
                          </span>
                          <span className="provider-preset-name">{preset.name}</span>
                          {preset.primePartner && (
                            <Heart size={12} className="provider-preset-badge provider-preset-badge--prime" />
                          )}
                          {preset.isPartner && !preset.primePartner && (
                            <Star size={12} className="provider-preset-badge provider-preset-badge--partner" />
                          )}
                          {currentPreset?.name === preset.name && (
                            <span className="provider-preset-active-label">{t("settings.current")}</span>
                          )}
                        </button>
                        {preset.websiteUrl && (
                          <a
                            href={preset.websiteUrl}
                            target="_blank"
                            onClick={(e) => {
                              e.preventDefault();
                              void openUrl(preset.websiteUrl!);
                            }}
                            rel="noopener noreferrer"
                            className="provider-preset-link"
                            aria-label={preset.name}
                          >
                            <ExternalLink size={13} />
                          </a>
                        )}
                      </div>

                      {expanded && (
                        <div className="provider-preset-detail">
                          {preset.descriptionKey && (
                            <p className="provider-preset-desc">{t(preset.descriptionKey)}</p>
                          )}
                          {preset.apiKeyUrl && (
                            <a
                              href={preset.apiKeyUrl}
                              target="_blank"
                              onClick={(e) => {
                                e.preventDefault();
                                void openUrl(preset.apiKeyUrl!);
                              }}
                              rel="noopener noreferrer"
                              className="provider-preset-apikey-link"
                            >
                              {t("settings.get_api_key")} <ExternalLink size={11} />
                            </a>
                          )}
                          <div className="provider-preset-models">
                            <span className="provider-preset-models-label">
                              {t("settings.recommended_models")}
                            </span>
                            {preset.models.length === 0 ? (
                              <p className="provider-preset-empty">{t("settings.no_models")}</p>
                            ) : (
                              preset.models.map((model) => (
                                <button
                                  key={model.model}
                                  type="button"
                                  className={`provider-preset-model ${
                                    model.recommended ? "is-recommended" : ""
                                  } ${currentModel === model.model ? "is-active" : ""}`}
                                  onClick={() => selectPreset(preset, model)}
                                >
                                  <span className="provider-preset-model-name">
                                    {model.displayName}
                                  </span>
                                  {model.description && (
                                    <span className="provider-preset-model-desc">{model.description}</span>
                                  )}
                                  {model.recommended && (
                                    <span className="provider-preset-model-badge">
                                      {t("settings.recommended")}
                                    </span>
                                  )}
                                </button>
                              ))
                            )}
                          </div>
                          <button
                            type="button"
                            className="btn btn-sm btn-primary provider-preset-use"
                            onClick={() => selectPreset(preset)}
                          >
                            {t("settings.use_provider")}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
