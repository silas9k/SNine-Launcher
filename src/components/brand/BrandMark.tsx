import type { HTMLAttributes } from "react";
import snineLogo from "../../assets/snine-logo.png";

export function BrandMark({ className = "", ...props }: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span className={`brand-mark ${className}`} {...props}>
      <img src={snineLogo} alt="" aria-hidden="true" />
    </span>
  );
}
