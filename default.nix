let
	renderers = import ./renderers.nix;

	supported_versions = builtins.mapAttrs (
		_engine: modules:
		builtins.map (module: module.version) modules
	) renderers;
in {
	inherit supported_versions;

	svg_to_png = {
		pkgs,
		debug ? false,
	}: import ./svg-to-png.nix {
		inherit debug pkgs;
	};

	svg_font_inliner = {
		pkgs,
		fontconfig,
		debug ? false,
	}: import ./svg-font-inliner.nix {
		inherit debug fontconfig pkgs;
	};

	diagram_generator = {
		pkgs,
		fontconfig,
		versions,
		debug ? false,
	}:
		let
			sandbox_run = import ./sandbox-run.nix { inherit debug pkgs; };

			resolve_engine_renderers = engine: modules:
				let
					requested_versions = if builtins.hasAttr engine versions then builtins.getAttr engine versions else [];
					module_by_version = builtins.listToAttrs (
						builtins.map (module: {
							name = module.version;
							value = module;
						}) modules
					);
				in
				builtins.map (
					version:
						if builtins.hasAttr version module_by_version
						then
							let
								module = builtins.getAttr version module_by_version;
							in {
								bin = module.bin;
								version = module.version;
								formats = module.formats;
								renderer = "${engine}-${module.version}";
							}
						else throw "unsupported renderer version '${version}' for engine '${engine}'"
				) requested_versions;

			available_renderers = builtins.mapAttrs resolve_engine_renderers renderers;

			validated_available_renderers = if pkgs.lib.allUnique
				(builtins.map
					(config: config.renderer)
					(builtins.concatLists (builtins.attrValues available_renderers))
				)
				then available_renderers
				else throw "renderer strings not unique";
		in
			(import ./diagram-generator.nix {
				inherit debug fontconfig pkgs sandbox_run validated_available_renderers;
			}).bin;
}
