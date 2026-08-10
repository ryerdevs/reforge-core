#include "Base.h"

CBase::CBase()
{
    id = 0;
}

CBase::~CBase()
{
}

bool CBase::isNumber() const
{
    return (id & MID_NUMBER);
}

bool CBase::isVar() const
{
    return (id & MID_VARIABLE);
}

bool CBase::isSymbol() const
{
    return (id & MID_SYMBOL);
}

