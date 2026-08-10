@echo off
SET outpath=.
if "%~1"=="" (
	echo "drag n drop the folder on this .bat"
) else (
	if exist "%~1" (
		echo Packing %~n1...
		PackMakerLite --nolog --parallel -p "%~n1"
		echo Moved %~n1 to %outpath%...
		move /Y "%~n1.eix" "%outpath%/%~n1.eix"
		move /Y "%~n1.epk" "%outpath%/%~n1.epk"
	) else (
		echo "%~1 not a folder to pack"
	)
)
REM pause
