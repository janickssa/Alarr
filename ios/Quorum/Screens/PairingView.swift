import QuorumAPI
import QuorumCore
import SwiftUI
import UIKit

/// Pair this iPhone: scan the QR code from Settings → Users on the web, or type the server
/// address and the 8-character code.
struct PairingView: View {
    @EnvironmentObject var model: AppModel
    var link: PairingLink?

    @State private var server = ""
    @State private var code = ""
    @State private var name = UIDevice.current.name
    @State private var scanning = false
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                VStack(alignment: .leading, spacing: 10) {
                    LogoMark(size: 48)
                    Text("Pair with Quorum").font(.largeTitle.weight(.medium))
                    Text("On your computer open Quorum → Settings → Users → Pair a device, then scan the code.")
                        .font(.callout).foregroundStyle(Theme.muted)
                }
                Button {
                    error = nil
                    scanning = true
                } label: {
                    Label("Scan QR code", systemImage: "qrcode.viewfinder").frame(maxWidth: .infinity, minHeight: 36)
                }
                .buttonStyle(.borderedProminent)

                VStack(alignment: .leading, spacing: 12) {
                    SectionTitle(text: "Or type it")
                    TextField("Server, e.g. quorum.your-tailnet.ts.net", text: $server)
                        .textContentType(.URL).keyboardType(.URL)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                        .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 10))
                    TextField("Code (XXXX-XXXX)", text: $code)
                        .font(.body.monospaced())
                        .textInputAutocapitalization(.characters).autocorrectionDisabled()
                        .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 10))
                    TextField("Device name", text: $name)
                        .padding(12).background(Theme.surface, in: RoundedRectangle(cornerRadius: 10))
                    Button {
                        Task { await submit() }
                    } label: {
                        Group {
                            if busy { ProgressView() } else { Text("Pair") }
                        }
                        .frame(maxWidth: .infinity, minHeight: 32)
                    }
                    .buttonStyle(.bordered)
                    .disabled(busy || server.isEmpty || code.count < 8)
                }

                if let error {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.callout).foregroundStyle(Theme.down)
                }
            }
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .sheet(isPresented: $scanning) {
            ZStack(alignment: .top) {
                QRScanner(
                    onCode: { text in
                        scanning = false
                        guard let l = PairingLink(text) else {
                            error = "That QR code isn't a Quorum pairing code."
                            return
                        }
                        server = l.server.absoluteString
                        code = l.code
                        Task { await submit() }
                    },
                    onError: { msg in
                        scanning = false
                        error = msg
                    }
                )
                .ignoresSafeArea()
                RoundedRectangle(cornerRadius: 18).stroke(Theme.accent, lineWidth: 3)
                    .frame(width: 240, height: 240).padding(.top, 180)
                Text("Point at the QR code on your computer")
                    .font(.callout).padding(10).background(.ultraThinMaterial, in: Capsule()).padding(.top, 40)
            }
            .presentationDragIndicator(.visible)
        }
        .onAppear {
            if let link {
                server = link.server.absoluteString
                code = link.code
                model.pendingPairing = nil
            }
        }
    }

    private func submit() async {
        guard let url = PairingLink.server(from: server) else {
            error = "Enter the server address, e.g. https://quorum.your-tailnet.ts.net"
            return
        }
        busy = true
        error = nil
        defer { busy = false }
        do {
            try await model.pair(server: url, code: code, deviceName: name.isEmpty ? "iPhone" : name)
            Haptics.success()
        } catch {
            Haptics.error()
            self.error = describe(error) == "Unexpected answer from the server"
                ? "Unknown or expired code. Start pairing again on your computer."
                : describe(error)
        }
    }
}
