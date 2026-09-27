// Ambient declaration: the `qrcode` package ships no types. printer.ts only
// needs the default export's shape (typed locally as `typeof QRCode`).
declare module 'qrcode' {
  const QRCode: any;
  export default QRCode;
}
