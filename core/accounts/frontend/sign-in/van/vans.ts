import stepModel from '../assets/login-van-step.glb?url';
import stepPoster from '../assets/login-van-step-poster.png?url';
import edvModel from '../assets/login-van-edv.glb?url';
import edvPoster from '../assets/login-van-edv-poster.png?url';

/** The vans the sign-in page picks from, one at random on each load. */
export const vans = [
  { model: stepModel, poster: stepPoster },
  { model: edvModel, poster: edvPoster },
];

export type Van = (typeof vans)[number];

export const randomVan = (): Van => vans[Math.floor(Math.random() * vans.length)]!;
