"use client";

import { useState } from "react";
import Reveal from "./Reveal";
import SectionHead from "./SectionHead";
import ArmPicker from "./ArmPicker";
import CompareNote from "./CompareNote";
import TaskPicker from "./TaskPicker";
import TranscriptPane from "./TranscriptPane";
import { TASKS, pct } from "./compare-data";

export default function Compare(): React.JSX.Element {
  const [taskId, setTaskId] = useState(TASKS[0].id);
  const [arm, setArm] = useState<string>("chisle");

  const task = TASKS.find((t) => t.id === taskId) ?? TASKS[0];
  const base = task.base;
  const cur = task.arms.get(arm) ?? task.chisle;
  const share = pct(cur.answerTokens, base.answerTokens);

  return (
    <section id="compare" className="py-28">
      <div className="mx-auto max-w-6xl px-5">
        <SectionHead eyebrow="Compare" title="Same prompt. Same model. Real transcripts.">
          Pick a task and an arm. Both panels are the{" "}
          <strong className="text-fg">verbatim committed output</strong> from the benchmark run,
          not a mock-up written for this page. The arms differ only in the injected system prompt.
        </SectionHead>

        <Reveal delay={0.06}>
          <TaskPicker selected={taskId} onSelect={setTaskId} />
          <p className="mt-5 border-l-2 border-line pl-4 text-sm italic text-dim">
            &ldquo;{task.prompt}&rdquo;
          </p>
        </Reveal>

        <Reveal delay={0.1}>
          <div className="mt-8 grid gap-4 lg:grid-cols-2">
            <TranscriptPane
              cell={base}
              frameClass="border border-line"
              headerClass=""
              bodyClass="text-dim"
            >
              <span className="text-sm font-semibold">bare model</span>
              <span className="font-mono text-xs text-dim">
                {base.answerTokens.toLocaleString()} answer tok · {base.lines} lines
              </span>
            </TranscriptPane>
            <TranscriptPane
              cell={cur}
              frameClass="border border-brand-hover/60 shadow-[0_0_40px_-12px_#ffa028]"
              headerClass="flex-wrap gap-x-3 gap-y-2"
              bodyClass=""
            >
              <ArmPicker task={task} selected={arm} onSelect={setArm} />
              <span className="shrink-0 font-mono text-xs">
                <span className={share > 100 ? "text-waste" : "text-accent"}>{share}%</span>
                <span className="text-dim">
                  {" "}
                  · {cur.answerTokens.toLocaleString()} answer tok · {cur.lines} lines
                </span>
              </span>
            </TranscriptPane>
          </div>
        </Reveal>

        <Reveal delay={0.14}>
          <CompareNote taskId={task.id} />
        </Reveal>
      </div>
    </section>
  );
}
