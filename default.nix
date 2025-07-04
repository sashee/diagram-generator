{
	required_plantuml_versions ? ["v1.2025.4" "v1.2025.3"],
	required_recharts_versions ? [],
}:
let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05";
  pkgs = import nixpkgs { config = {}; overlays = []; };

	available_renderers = {"plantuml" = (map 
		(version: {
			bin = (import (./plantuml + "/${version}.nix")).bin {version = version;};
			version = version;
		})
		(pkgs.lib.lists.naturalSort required_plantuml_versions));
	};

	bin = pkgs.writeShellScriptBin "diagram-generator" ''

export AVAILABLE_RENDERERS=$(cat <<'EOF'
${builtins.toJSON available_renderers}
EOF
)

case $1 in
    --list-available-renderers)

echo $(cat <<'EOF'
${builtins.toJSON
(map
	(engine: (
		map
			(renderer: ''${engine}-${renderer.version}'')
			(builtins.getAttr engine available_renderers)
	))
	(builtins.attrNames available_renderers))
}
EOF
)

;;
    *)

# no network access
${pkgs.landrun}/bin/landrun \
	--unrestricted-filesystem \
	--env AVAILABLE_RENDERERS \
${pkgs.nodejs_latest}/bin/node ${./src/index.ts} "$@"

;;
esac

	'';
in
	bin
