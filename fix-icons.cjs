const sharp = require('sharp');
const fs = require('fs');
const path = require('path');

const iconsDir = path.join(__dirname, 'src-tauri', 'icons');
const files = fs.readdirSync(iconsDir).filter(f => f.endsWith('.png'));

(async () => {
    console.log("Fixing icons to ensure strict RGBA format...");
    for (const file of files) {
        const filePath = path.join(iconsDir, file);
        try {
            // This forces the image to have an Alpha channel
            const buffer = await sharp(filePath).ensureAlpha().toFormat('png').toBuffer();
            fs.writeFileSync(filePath, buffer);
            console.log(`✅ Fixed ${file}`);
        } catch (e) {
            console.error(`❌ Failed to fix ${file}:`, e.message);
        }
    }
    console.log("Done! You can delete this script and run 'npm run tauri dev' again.");
})();