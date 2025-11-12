import type { Artifact } from "@a2a-js/sdk";
import { useState } from "react";

interface ArtifactPreviewProps {
  artifact: Artifact;
  onDownload?: () => void;
}

export default function ArtifactPreview({ artifact, onDownload }: ArtifactPreviewProps) {
  const [expanded, setExpanded] = useState(false);

  const handleDownload = () => {
    if (onDownload) {
      onDownload();
      return;
    }

    // Try to download from artifact data directly
    const part = artifact.parts?.[0];
    if (!part) return;

    let blob: Blob | null = null;
    let filename = artifact.name || "artifact";

    if ("file" in part && part.file) {
      const file = part.file;
      const mimeType = ("mimeType" in file && file.mimeType) || "application/octet-stream";

      if ("bytes" in file && file.bytes) {
        // Decode base64 bytes
        try {
          const binaryString = atob(file.bytes);
          const bytes = new Uint8Array(binaryString.length);
          for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i);
          }
          blob = new Blob([bytes], { type: mimeType });
        } catch (e) {
          console.error("Failed to decode artifact bytes:", e);
          return;
        }
      } else if ("uri" in file && file.uri) {
        // For URI, open in new tab
        window.open(file.uri, "_blank");
        return;
      }

      if ("name" in file && file.name) {
        filename = file.name;
      }
    } else if ("data" in part && part.data) {
      // Download JSON data
      const jsonString = JSON.stringify(part.data, null, 2);
      blob = new Blob([jsonString], { type: "application/json" });
      filename = filename.endsWith(".json") ? filename : `${filename}.json`;
    } else if ("text" in part && part.text) {
      // Download text
      blob = new Blob([part.text], { type: "text/plain" });
      filename = filename.endsWith(".txt") ? filename : `${filename}.txt`;
    }

    if (blob) {
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    }
  };

  // Extract content from artifact parts
  const getArtifactContent = () => {
    if (!artifact.parts || artifact.parts.length === 0) {
      return null;
    }

    const part = artifact.parts[0];

    if ("text" in part && part.text) {
      return { type: "text", content: part.text };
    }

    if ("data" in part && part.data) {
      return { type: "json", content: JSON.stringify(part.data, null, 2) };
    }

    if ("file" in part && part.file) {
      const file = part.file;
      if ("mimeType" in file) {
        const mimeType = file.mimeType || "application/octet-stream";

        if (mimeType.startsWith("image/")) {
          let src = "";
          if ("bytes" in file && file.bytes) {
            src = `data:${mimeType};base64,${file.bytes}`;
          } else if ("uri" in file && file.uri) {
            src = file.uri;
          }
          return { type: "image", content: src, mimeType };
        }

        if (mimeType === "application/json" || mimeType === "text/plain") {
          if ("bytes" in file && file.bytes) {
            try {
              const decoded = atob(file.bytes);
              return { type: mimeType === "application/json" ? "json" : "text", content: decoded };
            } catch (e) {
              return { type: "text", content: file.bytes };
            }
          }
        }

        return { type: "file", mimeType };
      }
    }

    return null;
  };

  const content = getArtifactContent();

  if (!content) {
    return (
      <div className="rounded-xl bg-slate-800/50 p-4 text-slate-400">
        <p className="text-sm">No preview available for this artifact</p>
        {artifact.name && <p className="text-xs text-slate-500 mt-1">{artifact.name}</p>}
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-slate-700 bg-slate-800/50 overflow-hidden">
      <div className="flex items-center justify-between border-b border-slate-700 px-4 py-2 bg-slate-900/50">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-slate-300">
            {artifact.name || "Artifact"}
          </span>
          {content.mimeType && (
            <span className="text-[10px] rounded-full bg-slate-700/50 px-2 py-0.5 text-slate-400">
              {content.mimeType}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {(content.type === "text" || content.type === "json") && (
            <button
              type="button"
              onClick={() => setExpanded(!expanded)}
              className="text-xs text-cyan-400 hover:text-cyan-300"
            >
              {expanded ? "Collapse" : "Expand"}
            </button>
          )}
          <button
            type="button"
            onClick={handleDownload}
            className="text-xs text-emerald-400 hover:text-emerald-300"
            title="Download artifact"
          >
            Download
          </button>
        </div>
      </div>

      <div className="p-4">
        {content.type === "image" && content.content && (
          <img
            src={content.content}
            alt={artifact.name || "Artifact image"}
            className="max-w-full h-auto rounded-lg"
          />
        )}

        {content.type === "json" && (
          <pre className={`text-xs text-slate-300 overflow-x-auto bg-slate-900/50 rounded-lg p-3 ${expanded ? "" : "max-h-32 overflow-y-hidden"}`}>
            {content.content}
          </pre>
        )}

        {content.type === "text" && (
          <div className={`text-sm text-slate-300 whitespace-pre-wrap ${expanded ? "" : "max-h-32 overflow-y-hidden line-clamp-6"}`}>
            {content.content}
          </div>
        )}

        {content.type === "file" && (
          <div className="text-sm text-slate-400">
            <p>Binary file ({content.mimeType})</p>
            <button
              type="button"
              onClick={handleDownload}
              className="mt-2 text-emerald-400 hover:text-emerald-300"
            >
              Click to download
            </button>
          </div>
        )}

        {artifact.description && (
          <p className="mt-3 text-xs text-slate-500 border-t border-slate-700 pt-2">
            {artifact.description}
          </p>
        )}
      </div>
    </div>
  );
}
