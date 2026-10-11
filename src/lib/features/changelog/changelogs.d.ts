/** The changelogs at the repository root, by file name ("/CHANGELOG.md"); each function
 * imports that file's text (`changelogs` in vite.config.ts). */
declare module 'virtual:changelogs' {
	const files: Record<string, () => Promise<string>>;
	export default files;
}
