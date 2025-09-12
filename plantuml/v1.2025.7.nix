{
	bin = {
		version,
		fontconfig
	}:
	let
		pkgs = import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05") {};
		wrapper = (import ./v1.2025.4.nix).makewrapper {
			inherit pkgs fontconfig;
			src = pkgs.fetchurl {
				url = "https://github.com/plantuml/plantuml/releases/download/${version}/plantuml-${builtins.substring 1 (-1) version}.jar";
				hash = "sha256-TtzdoWSkvi+PlU+ChoeVUA69SfQjBiNr+I6sQffiF6g=";
			};
		};
	in
		wrapper;
}

