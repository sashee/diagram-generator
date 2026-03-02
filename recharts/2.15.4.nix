let
	plantuml_1_2025_4 = import ../plantuml/v1.2025.4.nix;
	pkgs = plantuml_1_2025_4.pkgs;

	makewrapper = {pkgs, packageDir, wrapperScript}:
	let
		packageJson = pkgs.lib.importJSON (pkgs.lib.path.append packageDir "package.json");

		npmPackage = pkgs.buildNpmPackage {
			pname = ''${packageJson.name}-node_modules'';
			version = packageJson.version;
			src = (pkgs.lib.sources.cleanSourceWith {
				filter=name: type: builtins.elem name (builtins.map builtins.toString [(pkgs.lib.path.append packageDir "package.json") (pkgs.lib.path.append packageDir "package-lock.json")]);
				src=packageDir;
			});
			nodejs = pkgs.nodePackages_latest.nodejs;

			nativeBuildInputs = [
				pkgs.pkg-config
			];

			buildInputs = [
			];

			dontNpmBuild = true;

			#npmFlags = [ "--ignore-scripts" ];

			npmDeps = pkgs.importNpmLock {
				package = packageJson;
				packageLock = pkgs.lib.importJSON (pkgs.lib.path.append packageDir "package-lock.json");
			};

			npmConfigHook = pkgs.importNpmLock.npmConfigHook;
		};

		wrapper = pkgs.runCommand "recharts" {} ''
mkdir $out
ln -fs ${npmPackage}/lib/node_modules/*/node_modules $out/node_modules
cat << 'EOF' > $out/index.ts
#!${pkgs.nodePackages_latest.nodejs}/bin/node

${pkgs.lib.strings.fileContents wrapperScript}
EOF
chmod +x $out/index.ts
ln -fs ${(pkgs.lib.path.append packageDir "package.json")} $out/package.json
ln -fs ${(pkgs.lib.path.append packageDir "package-lock.json")} $out/package-lock.json
		'';

in
	''${wrapper}/index.ts'';

in
{
	version = "2.15.4";
	formats = [ "svg" ];
	makewrapper = makewrapper;
	bin = let
		wrapper = makewrapper {
			inherit pkgs;
			packageDir = ./2.15.4;
			wrapperScript = ./2.15.4/index.ts;
		};
	in
	wrapper;
}
