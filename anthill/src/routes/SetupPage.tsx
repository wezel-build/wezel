import { useState } from "react";
import { useAuth } from "../lib/AuthContext";
import { setupApi } from "../lib/api";

export default function SetupPage() {
  const { appSlug, githubHost: configuredHost } = useAuth();
  const [githubHost, setGithubHost] = useState("github.com");
  const [publicUrl, setPublicUrl] = useState(window.location.origin);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isComplete =
    new URLSearchParams(window.location.search).get("setup") === "complete";
  const hasApp = !!appSlug;

  const handleCreate = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await setupApi.getManifest(githubHost, publicUrl);

      // GitHub requires a form POST with the manifest as a field.
      const form = document.createElement("form");
      form.method = "POST";
      form.action = data.post_url;
      const input = document.createElement("input");
      input.type = "hidden";
      input.name = "manifest";
      input.value = JSON.stringify(data.manifest);
      form.appendChild(input);
      document.body.appendChild(form);
      form.submit();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create manifest");
      setLoading(false);
    }
  };

  if (hasApp || isComplete) {
    const host = configuredHost || githubHost;
    const slug = appSlug || "wezel";
    return (
      <div className="w-screen h-screen bg-[#0f0f0f] text-[#e8e4de] font-sans flex flex-col items-center justify-center">
        <div className="flex flex-col items-center gap-[32px] px-[40px] py-[48px] bg-[#1a1a1a] border border-[#2e2e2e] rounded-xl min-w-[380px]">
          <div className="flex items-center gap-[10px]">
            <img src="/wezel.svg" width={28} height={28} alt="wezel" />
            <span className="text-[22px] font-extrabold text-[#e07b39] tracking-[-0.5px] font-mono">
              wezel
            </span>
          </div>

          <div className="text-center flex flex-col gap-[6px]">
            <div className="text-[15px] font-semibold text-[#e8e4de]">
              GitHub App created
            </div>
            <div className="text-xs text-[#666] font-mono">
              Install the app on your organization to continue
            </div>
          </div>

          <a
            href={`https://${host}/apps/${slug}/installations/new`}
            className="flex items-center gap-[10px] px-[24px] py-[10px] bg-[#e8e4de] text-[#0f0f0f] rounded-[7px] no-underline text-[13px] font-bold font-mono tracking-[0.2px] transition-opacity duration-150 hover:opacity-85"
          >
            Install Wezel on GitHub
          </a>

          <a
            href="/"
            className="text-xs text-[#666] font-mono hover:text-[#e8e4de] transition-colors"
          >
            Continue to Wezel
          </a>
        </div>
      </div>
    );
  }

  return (
    <div className="w-screen h-screen bg-[#0f0f0f] text-[#e8e4de] font-sans flex flex-col items-center justify-center">
      <div className="flex flex-col items-center gap-[32px] px-[40px] py-[48px] bg-[#1a1a1a] border border-[#2e2e2e] rounded-xl min-w-[380px]">
        <div className="flex items-center gap-[10px]">
          <img src="/wezel.svg" width={28} height={28} alt="wezel" />
          <span className="text-[22px] font-extrabold text-[#e07b39] tracking-[-0.5px] font-mono">
            wezel
          </span>
        </div>

        <div className="text-center flex flex-col gap-[6px]">
          <div className="text-[15px] font-semibold text-[#e8e4de]">
            Initial Setup
          </div>
          <div className="text-xs text-[#666] font-mono">
            Create a GitHub App for this Wezel instance
          </div>
        </div>

        {error && (
          <div className="text-xs font-mono text-[#e07b39] bg-[#2a1a0f] border border-[#5a2e0a] rounded-md px-[14px] py-[8px] text-center">
            {error}
          </div>
        )}

        <div className="flex flex-col gap-[16px] w-full">
          <label className="flex flex-col gap-[4px]">
            <span className="text-xs text-[#666] font-mono">GitHub Host</span>
            <input
              value={githubHost}
              onChange={(e) => setGithubHost(e.target.value)}
              className="bg-[#0f0f0f] border border-[#2e2e2e] rounded-md px-[12px] py-[8px] text-[13px] font-mono text-[#e8e4de] outline-none focus:border-[#e07b39]"
              placeholder="github.com"
            />
          </label>

          <label className="flex flex-col gap-[4px]">
            <span className="text-xs text-[#666] font-mono">Public URL</span>
            <input
              value={publicUrl}
              onChange={(e) => setPublicUrl(e.target.value)}
              className="bg-[#0f0f0f] border border-[#2e2e2e] rounded-md px-[12px] py-[8px] text-[13px] font-mono text-[#e8e4de] outline-none focus:border-[#e07b39]"
              placeholder="https://wezel.example.com"
            />
          </label>
        </div>

        <button
          onClick={handleCreate}
          disabled={loading}
          className="flex items-center gap-[10px] px-[24px] py-[10px] bg-[#e8e4de] text-[#0f0f0f] rounded-[7px] text-[13px] font-bold font-mono tracking-[0.2px] transition-opacity duration-150 hover:opacity-85 disabled:opacity-50 cursor-pointer disabled:cursor-not-allowed border-none"
        >
          {loading ? "Redirecting to GitHub..." : "Create GitHub App"}
        </button>
      </div>
    </div>
  );
}
