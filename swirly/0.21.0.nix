{
	bin = {
		version,
	}:
	let
		pkgs = import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05") {};
		wrapper = (import ../recharts/2.15.4.nix).makewrapper {
			pkgs = pkgs;
			packageDir = ./0.21.0;
			wrapperScript = ./0.21.0/index.ts;
		};
	in
		wrapper;
}
