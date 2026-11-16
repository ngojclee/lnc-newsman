import { useMemo, useRef, useState, type ReactNode } from "react";

export interface AgentMarkdownProps {
  content: string;
  className?: string;
  /** Optional raw view toggle for power-user inspection */
  raw?: boolean;
  /** Optional article citation click handler (`#article-108`) for host reader navigation */
  onCitationClick?: (articleId: number) => void;
  /**
   * Optional monotonic Rich renderer hook (e.g. KaTeX / Mermaid) when the host
   * app bundles math/diagram libraries. Once triggered during streaming, the
   * component stays on the rich path without flickering back.
   */
  renderRichOverride?: (content: string) => ReactNode;
}

function escapeText(s: string): string {
  return s;
}

function isSafeHref(href: string): boolean {
  const trimmed = href.trim().toLowerCase();
  return (
    trimmed.startsWith("https://") ||
    trimmed.startsWith("http://") ||
    trimmed.startsWith("mailto:") ||
    trimmed.startsWith("/") ||
    trimmed.startsWith("#")
  );
}

/** Parse inline Markdown (`code`, **bold**, *italic*, ~~strike~~, [link](url), #article-N) safely into ReactNodes. */
function renderInline(
  text: string,
  keyPrefix: string,
  onCitationClick?: (articleId: number) => void,
): ReactNode[] {
  const nodes: ReactNode[] = [];
  const tokenRe =
    /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|~~[^~]+~~|\*[^*\n]+\*|_[^_\n]+_|\[[^\]]+\]\([^)]+\)|#article-\d+)/g;
  let lastIdx = 0;
  let match: RegExpExecArray | null;
  let i = 0;

  while ((match = tokenRe.exec(text)) !== null) {
    if (match.index > lastIdx) {
      nodes.push(escapeText(text.slice(lastIdx, match.index)));
    }
    const raw = match[0];
    const k = `${keyPrefix}-${i++}`;

    if (raw.startsWith("#article-")) {
      const id = Number(raw.replace("#article-", ""));
      if (onCitationClick && Number.isFinite(id) && id > 0) {
        nodes.push(
          <button
            key={k}
            type="button"
            className="agent-citation-link"
            onClick={() => onCitationClick(id)}
          >
            {raw}
          </button>,
        );
      } else {
        nodes.push(
          <code key={k} className="ak-md-inline-code">
            {raw}
          </code>,
        );
      }
    } else if (raw.startsWith("`") && raw.endsWith("`")) {
      nodes.push(
        <code key={k} className="ak-md-inline-code">
          {raw.slice(1, -1)}
        </code>,
      );
    } else if (
      (raw.startsWith("**") && raw.endsWith("**")) ||
      (raw.startsWith("__") && raw.endsWith("__"))
    ) {
      nodes.push(
        <strong key={k}>{renderInline(raw.slice(2, -2), k, onCitationClick)}</strong>,
      );
    } else if (raw.startsWith("~~") && raw.endsWith("~~")) {
      nodes.push(<del key={k}>{renderInline(raw.slice(2, -2), k, onCitationClick)}</del>);
    } else if (
      (raw.startsWith("*") && raw.endsWith("*")) ||
      (raw.startsWith("_") && raw.endsWith("_"))
    ) {
      nodes.push(<em key={k}>{renderInline(raw.slice(1, -1), k, onCitationClick)}</em>);
    } else if (raw.startsWith("[")) {
      const closeBracket = raw.indexOf("](");
      const label = raw.slice(1, closeBracket);
      const href = raw.slice(closeBracket + 2, -1).trim();
      if (isSafeHref(href)) {
        const citeMatch = /^#article-(\d+)$/.exec(href);
        if (citeMatch && onCitationClick) {
          const cid = Number(citeMatch[1]);
          nodes.push(
            <button
              key={k}
              type="button"
              className="agent-citation-link"
              onClick={() => onCitationClick(cid)}
            >
              {renderInline(label, k, onCitationClick)}
            </button>,
          );
        } else {
          nodes.push(
            <a
              key={k}
              href={href}
              target={href.startsWith("http") ? "_blank" : undefined}
              rel={href.startsWith("http") ? "noopener noreferrer" : undefined}
              className="ak-md-link"
            >
              {renderInline(label, k, onCitationClick)}
            </a>,
          );
        }
      } else {
        nodes.push(label);
      }
    } else {
      nodes.push(raw);
    }
    lastIdx = match.index + raw.length;
  }

  if (lastIdx < text.length) {
    nodes.push(escapeText(text.slice(lastIdx)));
  }
  return nodes;
}

function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      void navigator.clipboard.writeText(code).then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1600);
      });
    }
  };

  return (
    <div className="ak-md-codeblock">
      <div className="ak-md-codehdr">
        <span className="ak-md-codelang">{lang || "code"}</span>
        <button type="button" className="ak-md-copybtn" onClick={handleCopy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="ak-md-pre">
        <code>{code}</code>
      </pre>
    </div>
  );
}

/**
 * Mandatory Safe Markdown Renderer (`lnc-app-agents v2.3`):
 * - Prevents raw `**bold**` / `` `code` `` leaks in assistant & group chat bubbles
 * - Streaming-safe (gracefully renders unclosed triple-backtick fences mid-stream)
 * - Supports monotonic Simple -> Rich promotion when `renderRichOverride` is supplied
 */
export function AgentMarkdown({
  content,
  className = "",
  raw = false,
  onCitationClick,
  renderRichOverride,
}: AgentMarkdownProps) {
  const richLockedRef = useRef(false);

  const needsRich = useMemo(() => {
    if (!renderRichOverride) return false;
    if (richLockedRef.current) return true;
    const hasMathOrMermaid =
      /\$\$[\s\S]+?\$\$|\$[^$\n]+\$|```(?:mermaid|flow|seq|sequence)\b/i.test(content);
    if (hasMathOrMermaid) {
      richLockedRef.current = true;
      return true;
    }
    return false;
  }, [content, renderRichOverride]);

  const blocks = useMemo(() => {
    const src = (content || "").replace(/\r\n/g, "\n");
    const lines = src.split("\n");
    const out: ReactNode[] = [];
    let i = 0;
    let bKey = 0;

    while (i < lines.length) {
      const line = lines[i];

      // 1. Fenced code block (streaming-safe even if closing ``` hasn't arrived yet)
      if (line.trimStart().startsWith("```")) {
        const lang = line.trimStart().slice(3).trim();
        const codeLines: string[] = [];
        i += 1;
        while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
          codeLines.push(lines[i]);
          i += 1;
        }
        if (i < lines.length) i += 1;
        out.push(<CodeBlock key={`cb-${bKey++}`} lang={lang} code={codeLines.join("\n")} />);
        continue;
      }

      // 2. Blank line
      if (!line.trim()) {
        i += 1;
        continue;
      }

      // 3. Heading (# .. ######)
      const hMatch = /^(#{1,6})\s+(.+)$/.exec(line);
      if (hMatch) {
        const level = hMatch[1].length;
        const inner = renderInline(hMatch[2], `h-${bKey}`, onCitationClick);
        const k = `h-${bKey++}`;
        if (level === 1) out.push(<h1 key={k} className="ak-md-h1">{inner}</h1>);
        else if (level === 2) out.push(<h2 key={k} className="ak-md-h2">{inner}</h2>);
        else if (level === 3) out.push(<h3 key={k} className="ak-md-h3">{inner}</h3>);
        else out.push(<h4 key={k} className="ak-md-h4">{inner}</h4>);
        i += 1;
        continue;
      }

      // 4. Horizontal rule
      if (/^(?:---|\*\*\*|___)\s*$/.test(line.trim())) {
        out.push(<hr key={`hr-${bKey++}`} className="ak-md-hr" />);
        i += 1;
        continue;
      }

      // 5. Blockquote
      if (line.trimStart().startsWith(">")) {
        const qLines: string[] = [];
        while (i < lines.length && lines[i].trimStart().startsWith(">")) {
          qLines.push(lines[i].replace(/^\s*>\s?/, ""));
          i += 1;
        }
        out.push(
          <blockquote key={`bq-${bKey++}`} className="ak-md-quote">
            {renderInline(qLines.join(" "), `bqi-${bKey}`, onCitationClick)}
          </blockquote>,
        );
        continue;
      }

      // 6. GFM Table
      if (
        line.includes("|") &&
        i + 1 < lines.length &&
        /^\s*\|?[\s:-]+\|[\s|:-]*$/.test(lines[i + 1])
      ) {
        const parseRow = (r: string) =>
          r
            .trim()
            .replace(/^\|/, "")
            .replace(/\|$/, "")
            .split("|")
            .map((c) => c.trim());
        const headers = parseRow(line);
        i += 2;
        const rows: string[][] = [];
        while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
          rows.push(parseRow(lines[i]));
          i += 1;
        }
        const tk = `tbl-${bKey++}`;
        out.push(
          <div key={tk} className="ak-md-table-wrap">
            <table className="ak-md-table">
              <thead>
                <tr>
                  {headers.map((h, hi) => (
                    <th key={`${tk}-h-${hi}`}>{renderInline(h, `${tk}-hi-${hi}`, onCitationClick)}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map((r, ri) => (
                  <tr key={`${tk}-r-${ri}`}>
                    {r.map((cell, ci) => (
                      <td key={`${tk}-c-${ri}-${ci}`}>
                        {renderInline(cell, `${tk}-ci-${ri}-${ci}`, onCitationClick)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>,
        );
        continue;
      }

      // 7. Unordered list
      if (/^\s*[-*+]\s+/.test(line)) {
        const items: string[] = [];
        while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) {
          items.push(lines[i].replace(/^\s*[-*+]\s+/, ""));
          i += 1;
        }
        const lk = `ul-${bKey++}`;
        out.push(
          <ul key={lk} className="ak-md-ul">
            {items.map((it, idx) => (
              <li key={`${lk}-${idx}`}>{renderInline(it, `${lk}-i-${idx}`, onCitationClick)}</li>
            ))}
          </ul>,
        );
        continue;
      }

      // 8. Ordered list
      if (/^\s*\d+\.\s+/.test(line)) {
        const items: string[] = [];
        while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
          items.push(lines[i].replace(/^\s*\d+\.\s+/, ""));
          i += 1;
        }
        const ok = `ol-${bKey++}`;
        out.push(
          <ol key={ok} className="ak-md-ol">
            {items.map((it, idx) => (
              <li key={`${ok}-${idx}`}>{renderInline(it, `${ok}-i-${idx}`, onCitationClick)}</li>
            ))}
          </ol>,
        );
        continue;
      }

      // 9. Paragraph
      const pLines: string[] = [line];
      i += 1;
      while (
        i < lines.length &&
        lines[i].trim() !== "" &&
        !lines[i].trimStart().startsWith("```") &&
        !/^(#{1,6})\s+/.test(lines[i]) &&
        !/^\s*[-*+]\s+/.test(lines[i]) &&
        !/^\s*\d+\.\s+/.test(lines[i]) &&
        !lines[i].trimStart().startsWith(">")
      ) {
        pLines.push(lines[i]);
        i += 1;
      }
      const pk = `p-${bKey++}`;
      out.push(
        <p key={pk} className="ak-md-p">
          {renderInline(pLines.join(" "), pk, onCitationClick)}
        </p>,
      );
    }

    return out;
  }, [content, onCitationClick]);

  if (raw) {
    return <pre className={`ak-md-raw ${className}`.trim()}>{content}</pre>;
  }

  if (needsRich && renderRichOverride) {
    return <div className={`ak-md ${className}`.trim()}>{renderRichOverride(content)}</div>;
  }

  return <div className={`ak-md ${className}`.trim()}>{blocks}</div>;
}
export default AgentMarkdown;
