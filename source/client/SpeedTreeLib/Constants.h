///////////////////////////////////////////////////////////////////////
//	Constants
//
//	(c) 2003 IDV, Inc.
//
//	*** INTERACTIVE DATA VISUALIZATION (IDV) PROPRIETARY INFORMATION ***
//
//	This software is supplied under the terms of a license agreement or
//	nondisclosure agreement with Interactive Data Visualization and may
//	not be copied or disclosed except in accordance with the terms of
//	that agreement.
//
//      Copyright (c) 2001-2003 IDV, Inc.
//      All Rights Reserved.
//
//		IDV, Inc.
//		1233 Washington St. Suite 610
//		Columbia, SC 29201
//		Voice: (803) 799-1699
//		Fax:   (803) 931-0320
//		Web:   http://www.idvinc.com
//

#pragma once
//#include "LibVector_Source/IdvVector.h"
//#include "../Common Source/Filename.h"

// used to isolate SpeedGrass usage
#define USE_SPEEDGRASS

// paths
//const CFilename c_strTreePath = "../data/TheValley/Trees/";
//const CFilename c_strDataPath = "../data/TheValley/";

// vertex shader constant locations
constexpr	int		c_nShaderLightPosition = 16;
constexpr	int		c_nShaderGrassBillboard = 4;
constexpr	int		c_nShaderGrassLodParameters = 8;
constexpr	int		c_nShaderCameraPosition = 9;
constexpr	int		c_nShaderGrassWindDirection = 10;
constexpr	int		c_nShaderGrassPeriods = 11;
constexpr	int		c_nShaderLeafAmbient = 47;
constexpr	int		c_nShaderLeafDiffuse = 48;
constexpr	int		c_nShaderLeafSpecular = 49;
constexpr	int		c_nShaderLeafLightingAdjustment = 50;

// lighting
//const	float	c_afLightDirBumpMapping[3] = { 0.8f, 0.0f, 0.4f };
//const	float	c_afLightDir[3] = { 0.7434f, 0.0f, 0.57f };
//const	float	c_afLightPos[3] = { c_afLightDir[0] * 1000.0f, c_afLightDir[1] * 1000.0f, c_afLightDir[2] * 1000.0f };

// stats
//const	int		c_nStatFrameRate = 0;
//const	int		c_nStatDrawTime = 1;
//const	int		c_nStatSceneTriangles = 2;
//const	int		c_nStatTriangleRate = 3;
//const	int		c_nStatNone = 4;
//const	int		c_nStatSummary = 5;
//const	int		c_nStatCount = 6;

// grass
constexpr	float	c_fDefaultGrassFarLod = 300.0f;
constexpr	float	c_fGrassFadeLength = 50.0f;
constexpr	float	c_fMinBladeNoise = -0.2f;
constexpr	float	c_fMaxBladeNoise = 0.2f;
constexpr	float	c_fMinBladeThrow = 1.0f;
constexpr	float	c_fMaxBladeThrow = 2.5f;
constexpr	float	c_fMinBladeSize = 7.5f;
constexpr	float	c_fMaxBladeSize = 10.0f;
constexpr	int		c_nNumBladeMaps = 2;
constexpr	float	c_fGrassBillboardWideScalar = 1.75f;
constexpr	float	c_fWalkerHeight = 12.0f;
constexpr	int		c_nDefaultGrassBladeCount = 33000;
constexpr	int		c_nGrassRegionCount = 20;

// terrain
//const	float	c_fTerrainBaseRepeat = 1.0f;
//const	float	c_fTerrainDetail1Repeat = 80.0f;
//const	float	c_fTerrainDetail2Repeat = 8.0f;

// wind
//const	int		c_nNumWindMatrices = 4;                             // number of unique wind matrices
//const   int     c_nNumMatricesPerTree = 3;                          // number of individual matrices used per tree
//const	float	c_fShimmerSpeed = 50.0f;							// controls how fast the shimmer map rotates
//const	float	c_fShimmerExponent = 3.0f;							// controls linearity of shimmer speed

// lod
//const   float   c_fShadowCutoff = 5000.0f;                          // how far out shadows are no longer drawn (feet)
//const   float   c_fBillboardAlphaValue = 0.3294f;                   // value for glAlphaFunc() for single billboards
//const	float	c_fNearLod = 200.0f;
//const	float	c_fFarLod = 750.0f;

// misc
constexpr	float	c_fPi = 3.1415926535897932384626433832795f;
constexpr	float	c_fHalfPi = c_fPi * 0.5f;
constexpr	float	c_fQuarterPi = c_fPi * 0.25f;
constexpr	float	c_fTwoPi = 2.0f * c_fPi;
constexpr   float   c_fDeg2Rad = 57.29578f;                             // divide degrees by this to get radians
constexpr   float   c_f90 = 0.5f * c_fPi;

// branch vertex attribute sizes
//const	int		c_nBranchVertexTexture0Size = 4 * sizeof(float);	// normal map coordinates + wind index + wind weight
//const	int		c_nBranchVertexTexture1Size = 3 * sizeof(float);	// vertex tangents
//const	int		c_nBranchVertexTexture2Size = 3 * sizeof(float);	// vertex binormals
//const	int		c_nBranchVertexTexture3Size = 2 * sizeof(float);	// branch/frond shadow projection coordinates
//const	int		c_nBranchVertexNormalSize = 3 * sizeof(float);		// (nx, ny, nz)
//const	int		c_nBranchVertexPositionSize = 3 * sizeof(float);	// (x, y, z)
//
//const	int		c_nBranchVertexTotalSize = c_nBranchVertexTexture0Size +
//										   c_nBranchVertexTexture1Size +
//										   c_nBranchVertexTexture2Size +
//										   c_nBranchVertexTexture3Size +
//										   c_nBranchVertexNormalSize +
//										   c_nBranchVertexPositionSize;

// frond vertex attribute sizes
//const	int		c_nFrondVertexTexture0Size = 2 * sizeof(float);		// normal map coordinates
//const	int		c_nFrondVertexNormalSize = 3 * sizeof(float);		// (nx, ny, nz)
//const	int		c_nFrondVertexPositionSize = 3 * sizeof(float);		// (x, y, z)
//
//const	int		c_nFrondVertexTotalSize = c_nFrondVertexTexture0Size +
//										   c_nFrondVertexNormalSize +
//										   c_nFrondVertexPositionSize;

// leaf vertex attribute sizes
//const	int		c_nLeafVertexTexture0Size = 2 * sizeof(float);		// base map coordinate
//const	int		c_nLeafVertexTexture1Size = 2 * sizeof(float);		// shimmer map coordinates (s & t) + shimmer-effect index
//const	int		c_nLeafVertexNormalSize = 3 * sizeof(float);		// (nx, ny, nz)
//const	int		c_nLeafVertexPositionSize = 3 * sizeof(float);		// (x, y, z)
//
//const	int		c_nLeafVertexTotalSize = c_nLeafVertexTexture0Size +
//										 c_nLeafVertexTexture1Size +
//										 c_nLeafVertexNormalSize +
//										 c_nLeafVertexPositionSize;

// grass vertex attribute sizes
constexpr	int		c_nGrassVertexTexture0Size = 2 * sizeof(float);			// base map coordinate
constexpr	int		c_nGrassVertexTexture1Size = 4 * sizeof(float);			// vertex index, blade size, wind weight, noise factor
constexpr	int		c_nGrassVertexColorSize = 4 * sizeof(unsigned char);	// (rgba)
constexpr	int		c_nGrassVertexPositionSize = 3 * sizeof(float);			// (x, y, z)

constexpr	int		c_nGrassVertexTotalSize = c_nGrassVertexTexture0Size +
										  c_nGrassVertexTexture1Size +
										  c_nGrassVertexColorSize +
										  c_nGrassVertexPositionSize;

// terrain vertex attribute sizes
//const	int		c_nTerrainVertexTexture0Size = 2 * sizeof(float);
//const	int		c_nTerrainVertexTexture1Size = 2 * sizeof(float);
//const	int		c_nTerrainVertexTexture2Size = 2 * sizeof(float);
//const	int		c_nTerrainVertexPositionSize = 3 * sizeof(float);		// (x, y, z)
//const	int		c_nTerrainVertexTotalSize = c_nTerrainVertexTexture0Size +
//											c_nTerrainVertexTexture1Size +
//											c_nTerrainVertexTexture2Size +
//										    c_nTerrainVertexPositionSize;

// crosshair
//const    float      c_fCrosshairSize = 25.0f;
//const    float      c_fCrosshairAlpha = 0.075f;

