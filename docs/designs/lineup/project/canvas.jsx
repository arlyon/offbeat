/* Canvas — wraps all three phone variants in a design canvas */

function Phone({ children }) {
  return (
    <div style={{
      width: "100%",
      height: "100%",
      borderRadius: 0,
      overflow: "hidden",
      background: "var(--bg)",
      boxShadow: "0 8px 32px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.04)",
    }}>
      {children}
    </div>
  );
}

function Root() {
  return (
    <DesignCanvas>
      <DCSection
        id="festshell"
        title="App shell + festival search / favourites"
        subtitle="Three takes — by-the-book, motif-heavy, command-line. Each artboard is a working interactive prototype."
      >
        <DCArtboard id="a" label="A · Index — by-the-book" width={390} height={780}>
          <Phone><VariantA /></Phone>
        </DCArtboard>
        <DCArtboard id="b" label="B · Stub stack — ticket motif" width={390} height={780}>
          <Phone><VariantB /></Phone>
        </DCArtboard>
        <DCArtboard id="c" label="C · Console — terminal density" width={390} height={780}>
          <Phone><VariantC /></Phone>
        </DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<Root />);
