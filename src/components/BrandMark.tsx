import logoUrl from "../../design/xallauncher-portal.svg";

export function BrandMark({ large = false }: { large?: boolean }) {
  return <img className={`brand-mark${large ? " large" : ""}`} src={logoUrl} alt="" draggable={false} />;
}
