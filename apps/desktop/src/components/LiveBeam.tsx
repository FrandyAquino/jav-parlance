import { useReducedMotion } from "motion/react";
import { BorderBeam } from "./ui/border-beam";

/** Animated edge that signals live activity; omitted for reduced motion. */
export function LiveBeam({ active, tone = "record" }: { active: boolean; tone?: "record" | "accent" }) {
  const reduced = useReducedMotion();
  if (!active || reduced) return null;
  const [from, to] = tone === "record" ? ["var(--record)", "var(--accent)"] : ["var(--accent)", "#c08cff"];
  return <BorderBeam size={120} duration={7} colorFrom={from} colorTo={to} borderWidth={1} />;
}
