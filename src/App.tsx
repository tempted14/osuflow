import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "./lib/api";
import type {
  Beatmapset,
  DownloadProgressEvent,
  DownloadState,
  DownloadStateEvent,
  OsuInstall,
  SearchResponse,
  Settings,
} from "./lib/types";
import "./App.css";

// Phase 1 debug screen: proves search -> download -> import end to end. The real overlay UI
// replaces this in phase 2.

interface Job {
  state: DownloadState;
  received: number;
  total: number | null;
  error: string | null;
}

const NEW_JOB: Job = { state: "queued", received: 0, total: null, error: null };
const BUSY: DownloadState[] = ["queued", "downloading", "importing"];

const STATUS_CHOICES: [string, number][] = [
  ["Ranked", 1],
  ["Loved", 4],
  ["Qualified", 3],
  ["Pending", 0],
  ["Graveyard", -2],
];
const MODE_CHOICES: [string, number | null][] = [
  ["All modes", null],
  ["osu!", 0],
  ["taiko", 1],
  ["catch", 2],
  ["mania", 3],
];

const megabytes = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);

function jobLabel(job: Job): string {
  switch (job.state) {
    case "queued":
      return "Queued";
    case "downloading":
      return job.total
        ? `${Math.round((job.received / job.total) * 100)}% of ${megabytes(job.total)} MB`
        : `${megabytes(job.received)} MB`;
    case "importing":
      return "Sending to osu!…";
    case "done":
      return "Sent to osu! ✓";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
  }
}

function SetCard(props: {
  set: Beatmapset;
  job: Job | undefined;
  onDownload: () => void;
  onCancel: () => void;
}) {
  const { set, job } = props;
  const stars = set.beatmaps.map((b) => b.difficulty_rating);
  const range = stars.length
    ? `${Math.min(...stars).toFixed(2)}–${Math.max(...stars).toFixed(2)}★`
    : "";
  return (
    <li className="card">
      <img src={set.covers.list} alt="" loading="lazy" />
      <div className="meta">
        <strong>
          {set.artist} – {set.title}
        </strong>
        <span>
          mapped by {set.creator} · {set.status} · {set.beatmaps.length} diffs
          {range && ` · ${range}`}
          {set.video && " · video"}
        </span>
        {job?.state === "failed" && <span className="error">{job.error}</span>}
      </div>
      {job && BUSY.includes(job.state) ? (
        <button onClick={props.onCancel}>Cancel · {jobLabel(job)}</button>
      ) : (
        <button onClick={props.onDownload}>
          {job?.state === "done" ? "Download again" : "Download"}
        </button>
      )}
    </li>
  );
}

export default function App() {
  const [install, setInstall] = useState<OsuInstall | null | undefined>(undefined);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [text, setText] = useState("");
  const [status, setStatus] = useState(1);
  const [mode, setMode] = useState<number | null>(null);
  const [response, setResponse] = useState<SearchResponse | null>(null);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [jobs, setJobs] = useState<Record<number, Job>>({});
  const [manualId, setManualId] = useState("");
  const latestSearch = useRef(0);

  useEffect(() => {
    api.detectOsu().then(setInstall).catch(() => setInstall(null));
    api.getSettings().then(setSettings).catch(() => undefined);
  }, []);

  useEffect(() => {
    const listeners = Promise.all([
      listen<DownloadStateEvent>("download-state", ({ payload }) =>
        setJobs((all) => ({
          ...all,
          [payload.setId]: {
            ...(all[payload.setId] ?? NEW_JOB),
            state: payload.state,
            error: payload.error,
          },
        })),
      ),
      listen<DownloadProgressEvent>("download-progress", ({ payload }) =>
        setJobs((all) => ({
          ...all,
          [payload.setId]: {
            ...(all[payload.setId] ?? NEW_JOB),
            received: payload.received,
            total: payload.total,
          },
        })),
      ),
    ]);
    return () => {
      void listeners.then((unlisten) => unlisten.forEach((fn) => fn()));
    };
  }, []);

  // Debounced search; answers that arrive after a newer query was typed are ignored.
  useEffect(() => {
    const id = ++latestSearch.current;
    setLoading(true);
    const timer = setTimeout(() => {
      api
        .search({ query: text, mode, status, sort: "relevance", offset: 0, limit: 30 })
        .then((result) => {
          if (id !== latestSearch.current) return;
          setResponse(result);
          setSearchError(null);
        })
        .catch((error) => {
          if (id === latestSearch.current) setSearchError(String(error));
        })
        .finally(() => {
          if (id === latestSearch.current) setLoading(false);
        });
    }, 300);
    return () => clearTimeout(timer);
  }, [text, status, mode]);

  const download = (setId: number) => {
    const existing = jobs[setId];
    if (existing && BUSY.includes(existing.state)) return;
    setJobs((all) => ({ ...all, [setId]: NEW_JOB }));
    api.downloadBeatmapset(setId).catch((error) => {
      const message = String(error);
      const cancelled = message.includes("cancelled");
      setJobs((all) => ({
        ...all,
        [setId]: {
          ...(all[setId] ?? NEW_JOB),
          state: cancelled ? "cancelled" : "failed",
          error: cancelled ? null : message,
        },
      }));
    });
  };

  const toggleNoVideo = (noVideo: boolean) => {
    if (!settings) return;
    api.setSettings({ ...settings, noVideo }).then(setSettings).catch(() => undefined);
  };

  const manualSetId = Number(manualId);
  const manualValid = Number.isInteger(manualSetId) && manualSetId > 0;

  return (
    <main>
      <header>
        <h1>
          QuickDirect <small>phase 1 debug build</small>
        </h1>
        <p className="muted">
          {install === undefined ? (
            "Looking for osu!…"
          ) : install ? (
            <>
              osu! stable: <code>{install.exe}</code> ({install.source})
              {install.exclusiveFullscreen && " · exclusive fullscreen, overlays can't show over it"}
            </>
          ) : (
            "osu! stable not found - maps are saved but can't be imported yet."
          )}
        </p>
        <label>
          <input
            type="checkbox"
            checked={settings?.noVideo ?? true}
            disabled={!settings}
            onChange={(e) => toggleNoVideo(e.currentTarget.checked)}
          />{" "}
          Download without video
        </label>
      </header>

      <section className="controls">
        <input
          autoFocus
          placeholder="Search artist, title, mapper, tags…"
          value={text}
          onChange={(e) => setText(e.currentTarget.value)}
        />
        <select value={status} onChange={(e) => setStatus(Number(e.currentTarget.value))}>
          {STATUS_CHOICES.map(([label, value]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <select
          value={mode ?? ""}
          onChange={(e) => setMode(e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}
        >
          {MODE_CHOICES.map(([label, value]) => (
            <option key={label} value={value ?? ""}>
              {label}
            </option>
          ))}
        </select>
      </section>

      <form
        className="manual"
        onSubmit={(e) => {
          e.preventDefault();
          if (manualValid) download(manualSetId);
        }}
      >
        <input
          inputMode="numeric"
          placeholder="…or a beatmapset ID"
          value={manualId}
          onChange={(e) => setManualId(e.currentTarget.value)}
        />
        <button type="submit" disabled={!manualValid}>
          Download set
        </button>
        {manualValid && jobs[manualSetId] && (
          <span className="muted">
            {jobLabel(jobs[manualSetId])}
            {jobs[manualSetId].error && <span className="error"> {jobs[manualSetId].error}</span>}
          </span>
        )}
      </form>

      <p className="muted">
        {loading ? "Searching…" : response && `${response.results.length} results from ${response.mirror}`}
        {searchError && <span className="error"> {searchError}</span>}
      </p>
      <ul className="results">
        {response?.results.map((set) => (
          <SetCard
            key={set.id}
            set={set}
            job={jobs[set.id]}
            onDownload={() => download(set.id)}
            onCancel={() => void api.cancelDownload(set.id)}
          />
        ))}
      </ul>
    </main>
  );
}
