import { Github } from "lucide-react";
import { authApi } from "../lib/api";

export default function LoginPage({ forbidden }: { forbidden?: boolean }) {
  return (
    <div className="w-screen h-screen bg-[#0f0f0f] text-[#e8e4de] font-sans flex flex-col items-center justify-center">
      <div className="flex flex-col items-center gap-[32px] px-[40px] py-[48px] bg-[#1a1a1a] border border-[#2e2e2e] rounded-xl min-w-[320px]">
        {/* Logo + name */}
        <div className="flex items-center gap-[10px]">
          <img src="/wezel.svg" width={28} height={28} alt="wezel" />
          <span className="text-[22px] font-extrabold text-[#e07b39] tracking-[-0.5px] font-mono">
            wezel
          </span>
        </div>

        <div className="text-center flex flex-col gap-[6px]">
          <div className="text-[15px] font-semibold text-[#e8e4de]">
            Sign in to continue
          </div>
          <div className="text-xs text-[#666] font-mono">
            Authentication is required
          </div>
        </div>

        {forbidden && (
          <div className="text-xs font-mono text-[#e07b39] bg-[#2a1a0f] border border-[#5a2e0a] rounded-md px-[14px] py-[8px] text-center">
            You are not a member of the required GitHub organization.
          </div>
        )}

        <a
          href={authApi.loginUrl}
          className="flex items-center gap-[10px] px-[24px] py-[10px] bg-[#e8e4de] text-[#0f0f0f] rounded-[7px] no-underline text-[13px] font-bold font-mono tracking-[0.2px] transition-opacity duration-150 hover:opacity-85"
        >
          <Github size={16} />
          Sign in with GitHub
        </a>
      </div>
    </div>
  );
}
