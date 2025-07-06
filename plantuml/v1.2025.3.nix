{
	bin = {
		version
	}:
	let
		pkgs = import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05") {};
		wrapper = (import ./v1.2025.4.nix).makewrapper {
			pkgs = pkgs;
			src = pkgs.fetchurl {
				url = "https://github.com/plantuml/plantuml/releases/download/${version}/plantuml-${builtins.substring 1 (-1) version}.jar";
				hash = "sha256-g3t5Iv4wysEzb9V8WuzILwoGAyx325SaaYOppq5BcEo=";
			};
		};
	in
		wrapper;
}

