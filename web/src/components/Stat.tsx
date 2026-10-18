/** 增删行数，例如 +12 −3 */
export function Stat({ adds, dels }: { adds: number; dels: number }) {
  return (
    <>
      <span className="plus">+{adds}</span> <span className="minus">−{dels}</span>
    </>
  );
}
