/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { useMemo } from "react";
import { motion } from "motion/react";

const HIDDEN = { opacity: 0, y: 28 };
const SHOWN = { opacity: 1, y: 0 };
const VIEWPORT = { once: true, margin: "-80px" };
const EASE = [0.16, 1, 0.3, 1] as const;

export default function Reveal({
  children,
  delay = 0,
  className,
}: Readonly<{
  children: React.ReactNode;
  delay?: number;
  className?: string;
}>): React.JSX.Element {
  const transition = useMemo(() => ({ duration: 0.7, delay, ease: EASE }), [delay]);
  return (
    <motion.div
      className={className}
      initial={HIDDEN}
      whileInView={SHOWN}
      viewport={VIEWPORT}
      transition={transition}
    >
      {children}
    </motion.div>
  );
}
