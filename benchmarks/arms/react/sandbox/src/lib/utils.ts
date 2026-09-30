// The registry's `cn()` helper, the one support module a registry component imports
// (`import { cn } from "@/lib/utils"`). See ../../README.md, "Support stubs".
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
