// Download avatars so that the website can be rendered statically

import https from "https";
import fs from "fs";
import TWEETS from "./data/feedback";
import sharp from "sharp";

downloadTweets().catch(console.error);

// iterate over the TWEETS array and download the avatars
// for each tweet
async function downloadTweets() {
  TWEETS.forEach(async (tweet) => {
    const imageUrl = `https://github.com/${tweet.githubUsername}.png`;
    const imageName = `static/img/avatars/${tweet.githubUsername}.png`;

    await downloadImage(imageUrl, imageName);
    await resizeImage(imageName);
  });
}

function downloadImage(url, filepath) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (response) => {
        if (response.statusCode === 302 || response.statusCode === 301) {
          // Follow redirect and call the function recursively
          downloadImage(response.headers.location, filepath).then(resolve).catch(reject);
        } else {
          console.log(`Downloading ${url}`);
          const file = fs.createWriteStream(filepath);
          response.pipe(file);
          file.on("finish", () => {
            file.close();
            resolve(`Downloading ${url}`);
          });
        }
      })
      .on("error", (error) => {
        fs.unlink(filepath, () => {});
        reject(error.message);
      });
  });
}

async function resizeImage(imagePath) {
  console.log(`Resizing ${imagePath}`);
  await sharp(imagePath).resize(100, 100).toFile(imagePath.replace(".png", "-48x48.png"));
  // delete the original image
  fs.unlink(imagePath, () => {});
  // move the resized image to the original path
  fs.rename(imagePath.replace(".png", "-48x48.png"), imagePath, () => {});
}
